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
    digest: String,
    size: u64,
    /// Impronta dei primi `offset` byte, per ogni punto di lettura richiesto.
    prefixes: Vec<(u64, String)>,
}

/// Legge tutto il contenuto calcolando l'impronta finale e quelle dei prefissi richiesti.
fn scan(path: &Path, mut offsets: Vec<u64>) -> Result<Scan> {
    offsets.sort_unstable();
    offsets.dedup();
    let mut reader = open(path)?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut prefixes = Vec::new();
    let mut next = offsets.into_iter().peekable();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("{} non è un gzip valido", path.display()))?;
        if read == 0 {
            break;
        }
        let mut chunk = &buffer[..read];
        // Fotografa l'impronta a ogni punto di lettura che cade dentro questo blocco.
        while let Some(&offset) = next.peek() {
            if offset > size + chunk.len() as u64 {
                break;
            }
            let split = (offset - size) as usize;
            hash.update(&chunk[..split]);
            size += split as u64;
            chunk = &chunk[split..];
            prefixes.push((offset, format!("{:x}", hash.clone().finalize())));
            next.next();
        }
        hash.update(chunk);
        size += chunk.len() as u64;
        if size > MAX_DECOMPRESSED {
            bail!(
                "{} decompresso supera 4 GiB; non viene letto",
                path.display()
            );
        }
    }
    Ok(Scan {
        digest: format!("{:x}", hash.finalize()),
        size,
        prefixes,
    })
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
    let scan = scan(path, cursors.iter().map(|c| c.2).collect())?;
    if store.get_state(&done_key(&scan.digest))?.is_some() {
        return Ok(0);
    }

    // Cerca il file già letto di cui questo è la versione compressa: il più lungo
    // il cui contenuto archiviato coincide con l'inizio del file compresso.
    let mut best: Option<&(String, i64, u64)> = None;
    for candidate in &cursors {
        let (file_id, generation, offset) = candidate;
        if *offset > scan.size || best.is_some_and(|b| b.2 >= *offset) {
            continue;
        }
        let Some((_, prefix)) = scan.prefixes.iter().find(|(o, _)| o == offset) else {
            continue;
        };
        let archived = store.archived_prefix_digest(file_id, *generation, *offset)?;
        if archived.as_deref() == Some(prefix) {
            best = Some(candidate);
        }
    }
    let (file_id, mut cursor) = match best {
        Some((file_id, generation, offset)) => (
            file_id.clone(),
            Cursor {
                offset: *offset,
                generation: *generation,
                anchor: String::new(),
            },
        ),
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
            entries.push(Entry {
                key: format!("file:{file_id}:{}:{offset}", cursor.generation),
                file_id: Some(file_id.to_owned()),
                generation: cursor.generation,
                offset: offset as i64,
                event_ns,
                collected_ns: now,
                timestamp_origin: origin,
                raw: bytes.clone(),
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
        added += store.save_file_batch(file_id, cursor, &entries)?;
    }
}
