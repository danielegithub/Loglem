use crate::{
    archive::Archive,
    model::{Cursor, Entry},
    timestamps, util, BATCH_SIZE, MAX_LINE_BYTES,
};
use anyhow::{bail, Context, Result};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub fn identity(meta: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let birth = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}:{}:{birth}", meta.dev(), meta.ino())
    }
    #[cfg(not(unix))]
    {
        format!("{:?}:{}", meta.created(), meta.len())
    }
}

pub fn anchor(file: &mut File, offset: u64) -> Result<String> {
    let size = offset.min(256);
    file.seek(SeekFrom::Start(offset - size))?;
    let mut bytes = vec![0; size as usize];
    file.read_exact(&mut bytes)?;
    Ok(util::digest(&bytes))
}

/// Il file attivo più i file ruotati che corrispondono al modello, anche compressi
/// (`access-*.log` trova anche `access-1.log.gz`).
pub fn candidates(path: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    if let Some(pattern) = pattern {
        let plain = glob::Pattern::new(pattern)?;
        let compressed = glob::Pattern::new(&format!("{pattern}.gz"))?;
        for entry in fs::read_dir(path.parent().context("Missing source directory")?)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if (plain.matches(&name) || compressed.matches(&name)) && entry.file_type()?.is_file() {
                paths.push(entry.path());
            }
        }
    }
    paths.push(path.to_owned());
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn log_name(path: &Path) -> bool {
    let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
    name.ends_with(".log")
        || name.contains(".log.")
        || ["syslog", "messages", "cron", "secure", "maillog"]
            .iter()
            .any(|base| name == *base || name.starts_with(&format!("{base}.")))
}

/// Immediate children only; no symlinks, devices, or implicit recursive access.
pub fn folder_files(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.path().extension().and_then(|x| x.to_str()) == Some("log")
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// File `.log.gz` di una cartella: lo storico già ruotato e compresso.
pub fn compressed_in_folder(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.file_name().to_string_lossy().ends_with(".log.gz")
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// Legge un file di log, compresso o no.
pub fn collect_any(store: &mut Archive, path: &Path, now: i64) -> Result<usize> {
    if crate::sources::gzip::is_gzip(path) {
        crate::sources::gzip::collect(store, path, now)
    } else {
        collect(store, path, false, now)
    }
}

/// Reads bounded batches; an unfinished last line is left for the next poll.
pub fn collect(store: &mut Archive, path: &Path, complete: bool, now: i64) -> Result<usize> {
    let meta =
        fs::symlink_metadata(path).with_context(|| format!("Cannot read {}", path.display()))?;
    if !meta.is_file() {
        bail!("Source must be a regular file, not a symlink or device");
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() {
        bail!("Source is no longer a regular file");
    }
    let id = identity(&opened);
    if id != identity(&meta) {
        bail!("Source changed while opening it; retrying later");
    }
    let mut cursor = store.cursor(&id)?;
    let mut changed = false;
    if cursor.offset > opened.len()
        || (cursor.offset > 0 && anchor(&mut file, cursor.offset)? != cursor.anchor)
    {
        cursor = Cursor {
            generation: cursor.generation + 1,
            ..Cursor::default()
        };
        changed = true;
    }
    let snapshot_end = opened.len();
    let mut added = 0;
    while cursor.offset < snapshot_end {
        file.seek(SeekFrom::Start(cursor.offset))?;
        let mut reader = BufReader::new((&file).take(snapshot_end - cursor.offset));
        let mut entries = Vec::with_capacity(BATCH_SIZE);
        let mut batch_bytes = 0;
        let mut next_offset = cursor.offset;
        let (mut chain, mut lines) = (cursor.chain.clone(), cursor.lines);
        for _ in 0..BATCH_SIZE {
            let mut bytes = Vec::new();
            let count = reader
                .by_ref()
                .take((MAX_LINE_BYTES + 1) as u64)
                .read_until(b'\n', &mut bytes)?;
            if count == 0 {
                break;
            }
            if count > MAX_LINE_BYTES {
                bail!("A line exceeds 1 MiB; source preserved. Split oversized records at the producer");
            }
            if !bytes.ends_with(b"\n") && !complete {
                break;
            }
            let text = std::str::from_utf8(&bytes)
                .context("File is not UTF-8 text; source preserved")?
                .trim_end_matches(['\r', '\n'])
                .to_owned();
            let (event_ns, origin) = timestamps::event_time(&text, now);
            chain = util::chain_next(&chain, &bytes);
            lines += 1;
            entries.push(Entry {
                key: format!("file:{id}:{}:{next_offset}", cursor.generation),
                file_id: Some(id.clone()),
                generation: cursor.generation,
                offset: next_offset as i64,
                event_ns,
                collected_ns: now,
                timestamp_origin: origin,
                message: text,
            });
            next_offset += count as u64;
            batch_bytes += count;
            if batch_bytes >= 2 * 1024 * 1024 {
                break;
            }
        }
        drop(reader);
        if entries.is_empty() {
            break;
        }
        cursor.offset = next_offset;
        cursor.anchor = anchor(&mut file, cursor.offset)?;
        cursor.chain = chain;
        cursor.lines = lines;
        added += store.save_file_batch(&id, &cursor, &entries)?;
    }
    if changed {
        // Le righe scritte dopo l'ultima lettura e prima del troncamento sono perse.
        let last_read = store
            .get_state("last_success")?
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(now);
        store.record_gap(
            last_read,
            now,
            &format!(
                "{} è stato troncato o riscritto: le righe scritte dopo l'ultima lettura \
                 e prima del troncamento possono mancare",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
        )?;
    }
    Ok(added)
}

/// Catena di impronte, numero di righe e byte di un file, leggendolo tutto
/// (anche l'ultima riga senza a capo).
pub fn file_chain(path: &Path) -> Result<(String, u64, u64)> {
    let mut reader = BufReader::new(File::open(path)?);
    let (mut chain, mut lines, mut size) = (String::new(), 0u64, 0u64);
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok((chain, lines, size));
        }
        if count > MAX_LINE_BYTES {
            bail!("A line exceeds 1 MiB; source preserved");
        }
        chain = util::chain_next(&chain, &line);
        lines += 1;
        size += count as u64;
    }
}

/// Explicit handoff only: the caller guarantees that the producer has closed this file.
/// Normal collection NEVER deletes/truncates source files.
///
/// Prima di rimuovere l'originale controlla, byte per byte, che l'archivio lo contenga
/// tutto e che il file non sia cambiato nel frattempo.
pub fn archive_completed(
    store: &mut Archive,
    active: &Path,
    completed: &Path,
    remove: bool,
) -> Result<usize> {
    let active_meta = fs::metadata(active).ok();
    let before = fs::symlink_metadata(completed)?;
    if !before.is_file() {
        bail!("Completed file must be a regular file");
    }
    let id = identity(&before);
    if active == completed || active_meta.as_ref().is_some_and(|m| identity(m) == id) {
        bail!("Refusing cleanup of the active source file");
    }
    // Impronta dell'originale prima della lettura…
    let (chain_before, lines_before, size_before) = file_chain(completed)?;
    let count = collect(store, completed, true, timestamps::now_ns())?;
    // …e confronto con quella calcolata mentre si archiviava. Le righe scadute o mancanti
    // dall'archivio fanno fallire il controllo: l'originale resta.
    let cursor = store.cursor(&id)?;
    let archived_ok = cursor.offset == size_before
        && cursor.lines == lines_before
        && cursor.chain == chain_before
        && store.rows_of_file(&id, cursor.generation)? == lines_before;
    // …e ancora dopo, per accorgersi di un file cambiato mentre lo si leggeva.
    let unchanged = file_chain(completed)?.0 == chain_before;
    if !archived_ok || !unchanged {
        bail!("File content changed or archive verification failed; original preserved");
    }
    let after = fs::symlink_metadata(completed)?;
    if !after.is_file()
        || identity(&after) != id
        || before.len() != after.len()
        || before.modified()? != after.modified()?
    {
        bail!("File changed during archival; original preserved");
    }
    if remove {
        fs::remove_file(completed)?;
        util::sync_dir(completed.parent().context("Missing parent")?)?;
    }
    Ok(count)
}
