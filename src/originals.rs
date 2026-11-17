//! Cancellazione dai file originali: toglie le righe di una persona dai file di log *finiti*.
//!
//! Si riscrivono solo file che:
//! - non sono quello su cui un programma sta scrivendo (riscriverlo farebbe perdere righe nuove),
//! - non sono stati modificati da almeno 24 ore,
//! - sono già interamente nell'archivio di Loglem (il controllo è lo stesso della pulizia
//!   degli originali: catena di impronte e numero di righe).
//!
//! Il file nuovo si scrive accanto al vecchio, si controlla, poi lo sostituisce con un solo
//! passaggio atomico. Se qualcosa non torna, l'originale resta com'era. Il punto di lettura
//! dell'archivio viene spostato sul file nuovo, così Loglem non lo rilegge come se fosse nuovo.
//!
//! Non si può fare per Podman, Docker, Kubernetes e il journal: non sono file di Loglem.
use crate::{
    archive::Archive,
    management::SETTLE_TIME,
    model::{Cursor, Input, Source},
    query::Query,
    sources::{file, gzip},
    util, MAX_LINE_BYTES,
};
use anyhow::{bail, Context, Result};
use flate2::{write::GzEncoder, Compression};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Cosa è successo (o succederebbe) a un file.
#[derive(Debug, Clone, Serialize)]
pub struct FileOutcome {
    pub file: String,
    /// Righe che corrispondono.
    pub matched: u64,
    /// `rewritten`, `would_rewrite`, `nothing` oppure `skipped`.
    pub status: &'static str,
    /// Perché è stato saltato, in parole.
    pub note: String,
}

fn outcome(path: &Path, matched: u64, status: &'static str, note: &str) -> FileOutcome {
    FileOutcome {
        file: path.display().to_string(),
        matched,
        status,
        note: note.to_owned(),
    }
}

/// I file di una sorgente che si possono riscrivere (e quello in uso, da saltare).
fn files_of(source: &Source) -> Result<Option<(Vec<PathBuf>, Option<PathBuf>)>> {
    let (mut files, active) = match &source.input {
        Input::File {
            path,
            rotated_pattern,
        } => (
            file::candidates(path, rotated_pattern.as_deref())?,
            Some(path.clone()),
        ),
        Input::Folder { path } => {
            let mut files = file::folder_files(path)?;
            files.extend(file::compressed_in_folder(path)?);
            (files, None)
        }
        _ => return Ok(None),
    };
    if let Some(completed) = &source.completed_dir {
        files.extend(file::folder_files(completed)?);
        files.extend(file::compressed_in_folder(completed)?);
    }
    files.sort();
    files.dedup();
    Ok(Some((files, active)))
}

/// Per una sorgente: cosa succede ai suoi file originali. Con `apply` li riscrive davvero.
pub fn run(
    source: &Source,
    store: &mut Archive,
    query: &Query,
    apply: bool,
) -> Result<Vec<FileOutcome>> {
    let Some((files, active)) = files_of(source)? else {
        return Ok(vec![FileOutcome {
            file: "(sorgente)".into(),
            matched: 0,
            status: "skipped",
            note: "Podman, Docker, Kubernetes e journal non si possono riscrivere: \
                   limita la durata dei loro log, o ricrea il container"
                .into(),
        }]);
    };
    let mut outcomes = Vec::new();
    for path in files {
        let result = process(&path, active.as_deref(), store, query, apply)
            .unwrap_or_else(|error| outcome(&path, 0, "skipped", &format!("{error:#}")));
        outcomes.push(result);
    }
    Ok(outcomes)
}

fn process(
    path: &Path,
    active: Option<&Path>,
    store: &mut Archive,
    query: &Query,
    apply: bool,
) -> Result<FileOutcome> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Ok(outcome(path, 0, "skipped", "non è un file normale"));
    }
    if active == Some(path) {
        return Ok(outcome(
            path,
            0,
            "skipped",
            "è il file su cui scrive il programma: ripeti dopo la rotazione",
        ));
    }
    let age = SystemTime::now()
        .duration_since(meta.modified()?)
        .unwrap_or_default();
    if age < SETTLE_TIME {
        return Ok(outcome(
            path,
            0,
            "skipped",
            "modificato da meno di 24 ore: ripeti più tardi",
        ));
    }
    let compressed = gzip::is_gzip(path);
    // Il file deve essere già tutto nell'archivio: così nulla va perso né riletto.
    if !is_fully_archived(path, &meta, store, compressed)? {
        return Ok(outcome(
            path,
            0,
            "skipped",
            "non è ancora interamente archiviato: lo lascio com'è",
        ));
    }
    let matched = count_matches(path, query, compressed)?;
    if matched == 0 {
        return Ok(outcome(path, 0, "nothing", ""));
    }
    if !apply {
        return Ok(outcome(path, matched, "would_rewrite", ""));
    }
    if compressed {
        rewrite_gzip(path, &meta, store, query)?;
    } else {
        rewrite_plain(path, &meta, store, query)?;
    }
    Ok(outcome(path, matched, "rewritten", ""))
}

fn is_fully_archived(
    path: &Path,
    meta: &fs::Metadata,
    store: &Archive,
    compressed: bool,
) -> Result<bool> {
    if compressed {
        return gzip::is_archived(store, path);
    }
    let (chain, lines, size) = file::file_chain(path)?;
    let cursor = store.cursor(&file::identity(meta))?;
    Ok(cursor.offset == size && cursor.lines == lines && cursor.chain == chain)
}

/// Le righe del file, una alla volta, con il loro a capo.
fn for_each_line(reader: impl Read, mut each: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_LINE_BYTES {
            bail!("una riga supera 1 MiB");
        }
        each(&line)?;
    }
}

fn text_of(line: &[u8]) -> String {
    String::from_utf8_lossy(line)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

fn open_any(path: &Path, compressed: bool) -> Result<Box<dyn Read>> {
    Ok(if compressed {
        Box::new(gzip::open_reader(path)?)
    } else {
        Box::new(File::open(path)?)
    })
}

fn count_matches(path: &Path, query: &Query, compressed: bool) -> Result<u64> {
    let mut matched = 0;
    for_each_line(open_any(path, compressed)?, |line| {
        if query.matches(&text_of(line)) {
            matched += 1;
        }
        Ok(())
    })?;
    Ok(matched)
}

/// File temporaneo accanto all'originale, con gli stessi permessi (e proprietario, se possibile).
fn temp_beside(path: &Path, meta: &fs::Metadata) -> Result<(PathBuf, File)> {
    let name = path.file_name().context("nome del file mancante")?;
    let temp = path.with_file_name(format!(".{}.loglem-erase", name.to_string_lossy()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&temp)
        .with_context(|| format!("non posso creare {} (esiste già?)", temp.display()))?;
    fs::set_permissions(&temp, meta.permissions())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = std::os::unix::fs::chown(&temp, Some(meta.uid()), Some(meta.gid()));
    }
    Ok((temp, file))
}

/// Il file non deve essere cambiato mentre lo si riscriveva.
fn unchanged(path: &Path, before: &fs::Metadata) -> Result<bool> {
    let now = fs::symlink_metadata(path)?;
    Ok(now.len() == before.len()
        && now.modified()? == before.modified()?
        && file::identity(&now) == file::identity(before))
}

fn replace(temp: &Path, path: &Path) -> Result<()> {
    fs::rename(temp, path)?;
    util::sync_dir(path.parent().context("cartella mancante")?)?;
    Ok(())
}

fn rewrite_plain(
    path: &Path,
    meta: &fs::Metadata,
    store: &mut Archive,
    query: &Query,
) -> Result<()> {
    let old_id = file::identity(meta);
    let old_cursor = store.cursor(&old_id)?;
    let (temp, out) = temp_beside(path, meta)?;
    let result = (|| -> Result<()> {
        let mut out = BufWriter::new(out);
        let (mut chain, mut lines) = (String::new(), 0u64);
        for_each_line(File::open(path)?, |line| {
            if !query.matches(&text_of(line)) {
                out.write_all(line)?;
                chain = util::chain_next(&chain, line);
                lines += 1;
            }
            Ok(())
        })?;
        out.flush()?;
        out.get_ref().sync_all()?;
        // Il file scritto deve avere esattamente le righe che dovevano restare.
        let (written_chain, written_lines, _) = file::file_chain(&temp)?;
        if written_chain != chain || written_lines != lines {
            bail!("il file nuovo non corrisponde a quello atteso");
        }
        if !unchanged(path, meta)? {
            bail!("il file è cambiato mentre lo riscrivevo");
        }
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;

    // L'archivio segue il file nuovo.
    let new_meta = fs::symlink_metadata(path)?;
    let (chain, lines, size) = file::file_chain(path)?;
    let anchor = if size > 0 {
        file::anchor(&mut File::open(path)?, size)?
    } else {
        String::new()
    };
    store.rekey_file(
        &old_id,
        &file::identity(&new_meta),
        &Cursor {
            offset: size,
            generation: old_cursor.generation,
            anchor,
            chain,
            lines,
        },
    )
}

fn rewrite_gzip(
    path: &Path,
    meta: &fs::Metadata,
    store: &mut Archive,
    query: &Query,
) -> Result<()> {
    let old_digest = gzip::content_digest(path)?;
    let (temp, out) = temp_beside(path, meta)?;
    let result = (|| -> Result<String> {
        let mut encoder = GzEncoder::new(BufWriter::new(out), Compression::default());
        for_each_line(gzip::open_reader(path)?, |line| {
            if !query.matches(&text_of(line)) {
                encoder.write_all(line)?;
            }
            Ok(())
        })?;
        let mut writer = encoder.finish()?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        // Il file compresso nuovo deve potersi rileggere per intero.
        let new_digest = gzip::content_digest(&temp)?;
        if !unchanged(path, meta)? {
            bail!("il file è cambiato mentre lo riscrivevo");
        }
        replace(&temp, path)?;
        Ok(new_digest)
    })();
    let new_digest = match result {
        Ok(digest) => digest,
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
    };
    // Il contenuto nuovo conta come già archiviato: non va importato un'altra volta.
    gzip::alias_done(store, &old_digest, &new_digest)
}
