//! File di log ruotati e compressi (`.gz`).
//!
//! Un file compresso non cambia più: viene letto una volta sola. Prima di importarlo,
//! Loglem controlla se il suo inizio coincide con un file già letto quando non era
//! ancora compresso (per esempio `access.log` prima che Caddy lo ruotasse e comprimesse):
//! in quel caso importa solo la parte mancante, senza duplicare righe. Altrimenti è
//! storico precedente a Loglem e viene importato tutto.
use crate::{
    archive::Archive,
    model::{Cursor, Entry},
    timestamps, util, BATCH_SIZE, MAX_LINE_BYTES,
};
use anyhow::{bail, Context, Result};
use flate2::read::MultiGzDecoder;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::Path,
};

/// Protezione dai file che si espandono a dismisura una volta decompressi.
const MAX_DECOMPRESSED: u64 = 4 * 1024 * 1024 * 1024;

pub fn is_gzip(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("gz")
}

fn open(path: &Path) -> Result<impl Read> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        bail!("{} non è un file normale", path.display());
    }
    let file =
        File::open(path).with_context(|| format!("Non riesco a leggere {}", path.display()))?;
    Ok(MultiGzDecoder::new(file).take(MAX_DECOMPRESSED + 1))
}

/// Risultato della lettura completa del contenuto decompresso.
struct Scan {
    /// Impronta dell'intero contenuto: riconosce un file già importato.
    digest: String,
    size: u64,
    /// Catena e numero di righe ai punti di lettura richiesti: (byte, catena, righe).
    prefixes: Vec<(u64, String, u64)>,
}

/// Legge tutto il contenuto, riga per riga, calcolando l'impronta finale e la catena
/// ai punti di lettura indicati (che sono sempre fine di riga).
fn scan(path: &Path, offsets: Vec<u64>) -> Result<Scan> {
    let wanted: std::collections::HashSet<u64> = offsets.into_iter().collect();
    let mut reader = BufReader::new(open(path)?);
    let mut whole = Sha256::new();
    let (mut chain, mut lines, mut size) = (String::new(), 0u64, 0u64);
    let mut prefixes = Vec::new();
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .with_context(|| format!("{} non è un gzip valido", path.display()))?;
        if count == 0 {
            break;
        }
        if count > MAX_LINE_BYTES {
            bail!("Una riga supera 1 MiB in {}", path.display());
        }
        whole.update(&line);
        chain = util::chain_next(&chain, &line);
        lines += 1;
        size += count as u64;
        if size > MAX_DECOMPRESSED {
            bail!(
                "{} decompresso supera 4 GiB; non viene letto",
                path.display()
            );
        }
        if wanted.contains(&size) {
            prefixes.push((size, chain.clone(), lines));
        }
    }
    Ok(Scan {
        digest: format!("{:x}", whole.finalize()),
        size,
        prefixes,
    })
}

/// Il contenuto decompresso di un `.gz`, da leggere riga per riga.
pub fn open_reader(path: &Path) -> Result<impl Read> {
    open(path)
}

/// Dopo aver riscritto un `.gz`: il contenuto nuovo vale come già archiviato, come il vecchio.
pub fn alias_done(store: &Archive, old_digest: &str, new_digest: &str) -> Result<()> {
    if let Some(value) = store.get_state(&done_key(old_digest))? {
        store.set_state(&done_key(new_digest), &value)?;
    }
    Ok(())
}

fn done_key(digest: &str) -> String {
    format!("gz:{digest}")
}

/// Impronta del contenuto decompresso.
pub fn content_digest(path: &Path) -> Result<String> {
    Ok(scan(path, Vec::new())?.digest)
}

/// Vero se il contenuto di questo `.gz` è già tutto nell'archivio.
pub fn is_archived(store: &Archive, path: &Path) -> Result<bool> {
    Ok(store
        .get_state(&done_key(&content_digest(path)?))?
        .is_some())
}

/// Importa il contenuto del file compresso che non è ancora nell'archivio.
pub fn collect(store: &mut Archive, path: &Path, now: i64) -> Result<usize> {
    let cursors = store.file_cursors()?;
    let scan = scan(path, cursors.iter().map(|c| c.offset).collect())?;
    if store.get_state(&done_key(&scan.digest))?.is_some() {
        return Ok(0);
    }

    // Cerca il file già letto di cui questo è la versione compressa: il più lungo il cui
    // inizio coincide, riga per riga, con quello compresso e che è ancora tutto nell'archivio.
    let mut best: Option<Cursor> = None;
    let mut best_id = String::new();
    for candidate in &cursors {
        if candidate.offset > scan.size
            || best.as_ref().is_some_and(|b| b.offset >= candidate.offset)
        {
            continue;
        }
        let same_start = scan.prefixes.iter().any(|(o, c, l)| {
            *o == candidate.offset && *c == candidate.chain && *l == candidate.lines
        });
        if same_start
            && store.rows_of_file(&candidate.file_id, candidate.generation)? == candidate.lines
        {
            best_id = candidate.file_id.clone();
            best = Some(Cursor {
                offset: candidate.offset,
                generation: candidate.generation,
                anchor: String::new(),
                chain: candidate.chain.clone(),
                lines: candidate.lines,
            });
        }
    }
    let (file_id, mut cursor) = match best {
        Some(found) => (best_id, found),
        None => (format!("gz:{}", scan.digest), Cursor::default()),
    };

    let added = import(store, path, &file_id, &mut cursor, now)?;
    store.set_state(&done_key(&scan.digest), &file_id)?;
    Ok(added)
}

/// Legge le righe da `cursor.offset` in poi e le salva con lo stesso schema dei file.
fn import(
    store: &mut Archive,
    path: &Path,
    file_id: &str,
    cursor: &mut Cursor,
    now: i64,
) -> Result<usize> {
    let mut reader = BufReader::new(open(path)?);
    std::io::copy(
        &mut reader.by_ref().take(cursor.offset),
        &mut std::io::sink(),
    )?;
    let mut added = 0;
    let mut offset = cursor.offset;
    let (mut chain, mut lines) = (cursor.chain.clone(), cursor.lines);
    loop {
        let mut entries = Vec::with_capacity(BATCH_SIZE);
        let mut last = Vec::new();
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
                bail!("Una riga supera 1 MiB in {}", path.display());
            }
            let text = std::str::from_utf8(&bytes)
                .with_context(|| format!("{} non contiene testo UTF-8", path.display()))?
                .trim_end_matches(['\r', '\n'])
                .to_owned();
            let (event_ns, origin) = timestamps::event_time(&text, now);
            chain = util::chain_next(&chain, &bytes);
            lines += 1;
            entries.push(Entry {
                key: format!("file:{file_id}:{}:{offset}", cursor.generation),
                file_id: Some(file_id.to_owned()),
                generation: cursor.generation,
                offset: offset as i64,
                event_ns,
                collected_ns: now,
                timestamp_origin: origin,
                message: text,
            });
            offset += count as u64;
            last = bytes;
        }
        if entries.is_empty() {
            return Ok(added);
        }
        cursor.offset = offset;
        cursor.anchor = util::digest(&last);
        cursor.chain = chain.clone();
        cursor.lines = lines;
        added += store.save_file_batch(file_id, cursor, &entries)?;
    }
}
