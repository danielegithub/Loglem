//! Modalità prova: l'archivio di una sorgente viene sigillato giorno per giorno.
//!
//! Per ogni giorno UTC concluso si calcola un'impronta SHA-256 del testo delle righe raccolte quel
//! giorno, concatenata all'impronta del giorno precedente: cambiare o togliere un giorno
//! rompe tutti i successivi. Facoltativamente ogni impronta riceve una marca temporale
//! RFC 3161 da un'autorità esterna (che vede solo l'impronta, mai i log).
//!
//! Cosa dimostra: che le righe di un giorno sigillato non sono cambiate dopo il sigillo
//! (e, con la marca temporale, dopo l'ora della marca). Non dimostra che fossero corrette
//! prima: Loglem le ha ricevute così dal programma che le ha scritte.
use crate::{archive::Archive, config, model::Source, util};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const DAY: i64 = 86_400 * 1_000_000_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    /// Autorità di marca temporale RFC 3161 (es. https://freetsa.org/tsr); vuoto = nessuna.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tsa: Option<String>,
}

/// Una riga del registro: un giorno sigillato, oppure la sua scadenza.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Record {
    Sealed {
        day: String,
        count: i64,
        hash: String,
        previous: String,
        sealed_at: String,
    },
    Expired {
        day: String,
        at: String,
    },
    /// Righe tolte da giorni già sigillati (cancellazione su richiesta): le impronte dei giorni
    /// toccati e dei successivi sono state ricalcolate. Il registro non si riscrive: si aggiunge.
    Amended {
        at: String,
        reason: String,
        rows_removed: i64,
        changes: Vec<Change>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Change {
    day: String,
    count: i64,
    /// Righe tolte da questo giorno (0 = impronta ricalcolata solo perché un giorno prima è cambiato).
    #[serde(default)]
    removed: i64,
    old_hash: String,
    new_hash: String,
    previous: String,
}

/// Un giorno sigillato, come risulta dopo aver riletto tutto il registro.
#[derive(Debug, Clone)]
struct Day {
    day: String,
    count: i64,
    hash: String,
    previous: String,
}

/// La nota di una modifica, come la mostra la verifica.
#[derive(Debug, Clone)]
struct Note {
    at: String,
    reason: String,
    rows_removed: i64,
    old_hash: String,
    new_hash: String,
}

/// Il registro riletto: giorni con i valori attuali, giorni scaduti, impronte sostituite
/// e note delle modifiche di ogni giorno.
#[derive(Default)]
struct Chain {
    days: Vec<Day>,
    expired: std::collections::HashSet<String>,
    retired: std::collections::HashSet<String>,
    notes: std::collections::HashMap<String, Vec<Note>>,
}

fn replay(records: &[Record]) -> Chain {
    let mut chain = Chain::default();
    for record in records {
        match record {
            Record::Sealed {
                day,
                count,
                hash,
                previous,
                ..
            } => chain.days.push(Day {
                day: day.clone(),
                count: *count,
                hash: hash.clone(),
                previous: previous.clone(),
            }),
            Record::Expired { day, .. } => {
                chain.expired.insert(day.clone());
            }
            Record::Amended {
                at,
                reason,
                changes,
                ..
            } => {
                for change in changes {
                    if let Some(day) = chain.days.iter_mut().find(|d| d.day == change.day) {
                        chain.retired.insert(day.hash.clone());
                        day.hash = change.new_hash.clone();
                        day.previous = change.previous.clone();
                        day.count = change.count;
                    }
                    chain
                        .notes
                        .entry(change.day.clone())
                        .or_default()
                        .push(Note {
                            at: at.clone(),
                            reason: reason.clone(),
                            rows_removed: change.removed,
                            old_hash: change.old_hash.clone(),
                            new_hash: change.new_hash.clone(),
                        });
                }
            }
        }
    }
    chain
}

fn day_start(ns: i64) -> i64 {
    ns.div_euclid(DAY) * DAY
}

fn day_name(start: i64) -> String {
    crate::timestamps::format_ns(start)[..10].to_owned()
}

fn day_from_name(name: &str) -> Result<i64> {
    crate::timestamps::parse_date(name, false)
}

pub fn chain_path(dir: &Path, source: &str) -> PathBuf {
    dir.join("proof").join(format!("{source}.chain"))
}

fn tsa_path(dir: &Path, source: &str) -> PathBuf {
    dir.join("proof").join(format!("{source}.tsa"))
}

fn read_records(path: &Path) -> Result<Vec<Record>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).context("Registro della prova danneggiato"))
        .collect()
}

fn append(path: &Path, line: &str) -> Result<()> {
    util::private_dir(path.parent().context("cartella mancante")?)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    file.sync_all()?;
    Ok(())
}

/// Impronta di un giorno: l'impronta precedente seguita da ogni riga, in ordine di arrivo.
fn day_hash(store: &Archive, start: i64, previous: &str) -> Result<(String, i64)> {
    let mut hash = Sha256::new();
    hash.update(previous.as_bytes());
    let mut select = store.db.prepare(
        "SELECT id, event_ns, collected_ns, message FROM entries \
         WHERE collected_ns >= ?1 AND collected_ns < ?2 ORDER BY id",
    )?;
    let mut rows = select.query(params![start, start + DAY])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        let (id, event, collected): (i64, i64, i64) = (row.get(0)?, row.get(1)?, row.get(2)?);
        let message: String = row.get(3)?;
        hash.update(id.to_be_bytes());
        hash.update(event.to_be_bytes());
        hash.update(collected.to_be_bytes());
        hash.update((message.len() as u64).to_be_bytes());
        hash.update(message.as_bytes());
        count += 1;
    }
    Ok((format!("{:x}", hash.finalize()), count))
}

fn last_sealed(records: &[Record]) -> Option<(i64, String)> {
    let chain = replay(records);
    let last = chain.days.last()?;
    Some((day_from_name(&last.day).ok()?, last.hash.clone()))
}

/// Sigilla i giorni conclusi non ancora sigillati. Da chiamare a ogni giro di raccolta.
pub fn seal(dir: &Path, source: &Source, store: &Archive, now: i64) -> Result<usize> {
    let Some(proof) = &source.proof else {
        return Ok(0);
    };
    let path = chain_path(dir, &source.id);
    let records = read_records(&path)?;
    let today = day_start(now);
    let (mut next_day, mut previous) = match last_sealed(&records) {
        Some((day, hash)) => (day + DAY, hash),
        None => {
            // Si parte dal primo giorno con righe raccolte.
            let first: Option<i64> =
                store
                    .db
                    .query_row("SELECT min(collected_ns) FROM entries", [], |r| r.get(0))?;
            (day_start(first.unwrap_or(today)), "inizio".to_owned())
        }
    };
    let mut sealed = 0;
    while next_day < today {
        let (hash, count) = day_hash(store, next_day, &previous)?;
        let record = Record::Sealed {
            day: day_name(next_day),
            count,
            hash: hash.clone(),
            previous: previous.clone(),
            sealed_at: crate::timestamps::format_ns(now),
        };
        append(&path, &serde_json::to_string(&record)?)?;
        if let Some(url) = &proof.tsa {
            // Se l'autorità non risponde si riprova al giro successivo.
            if let Err(error) = request_timestamp(dir, &source.id, &day_name(next_day), &hash, url)
            {
                crate::diagnostics::error("proof", "timestamp", Some(&source.id), &error);
            }
        }
        previous = hash;
        next_day += DAY;
        sealed += 1;
    }
    if let Some(url) = &proof.tsa {
        retry_timestamps(dir, &source.id, &records, url);
    }
    Ok(sealed)
}

/// In modalità prova si cancellano solo giorni interi già sigillati, e ogni scadenza
/// viene scritta nel registro: la verifica distingue un giorno scaduto da uno tolto.
pub fn expire(dir: &Path, source: &Source, store: &Archive, cutoff: i64, now: i64) -> Result<()> {
    let path = chain_path(dir, &source.id);
    let records = read_records(&path)?;
    let Some((last, _)) = last_sealed(&records) else {
        return Ok(()); // nulla di sigillato: non si cancella niente
    };
    // Solo giorni interi, solo se sigillati.
    let limit = day_start(cutoff).min(last + DAY);
    let expired: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            Record::Expired { day, .. } => Some(day.clone()),
            _ => None,
        })
        .collect();
    for record in &records {
        if let Record::Sealed { day, .. } = record {
            let start = day_from_name(day)?;
            if start + DAY <= limit && !expired.contains(day) {
                store.db.execute(
                    "DELETE FROM entries WHERE collected_ns >= ?1 AND collected_ns < ?2",
                    params![start, start + DAY],
                )?;
                let line = Record::Expired {
                    day: day.clone(),
                    at: crate::timestamps::format_ns(now),
                };
                append(&path, &serde_json::to_string(&line)?)?;
            }
        }
    }
    Ok(())
}

// ---------- marca temporale RFC 3161 ----------

/// Richiesta DER minima: versione 1, impronta SHA-256, certificato dell'autorità incluso.
fn timestamp_request(hash: &[u8; 32]) -> Vec<u8> {
    let mut request = vec![
        0x30, 0x39, // TimeStampReq
        0x02, 0x01, 0x01, // versione 1
        0x30, 0x31, // MessageImprint
        0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
        0x00, // sha256
        0x04, 0x20, // 32 byte
    ];
    request.extend_from_slice(hash);
    request.extend_from_slice(&[0x01, 0x01, 0xff]); // certReq = true
    request
}

/// Chiede la marca e la salva solo se l'autorità l'ha concessa per questa impronta.
fn request_timestamp(dir: &Path, source: &str, day: &str, hash: &str, url: &str) -> Result<()> {
    let digest: [u8; 32] = hex(hash)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("impronta non valida"))?;
    let response = ureq::post(url)
        .set("Content-Type", "application/timestamp-query")
        .timeout(std::time::Duration::from_secs(30))
        .send_bytes(&timestamp_request(&digest))
        .map_err(|e| anyhow::anyhow!("Autorità di marca temporale non raggiungibile: {e}"))?;
    let mut body = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(response.into_reader(), 1024 * 1024),
        &mut body,
    )?;
    // TimeStampResp: SEQUENCE { PKIStatusInfo SEQUENCE { INTEGER 0 = concessa } … }
    let granted = body.windows(3).take(16).any(|w| w == [0x02, 0x01, 0x00]);
    let ours = body.windows(32).any(|w| w == digest);
    if !granted || !ours {
        bail!("L'autorità non ha concesso la marca per questa impronta");
    }
    let line = serde_json::json!({"day": day, "hash": hash, "token": STANDARD.encode(&body)});
    append(&tsa_path(dir, source), &line.to_string())
}

/// Le impronte che hanno già una marca, come «giorno:impronta».
fn stamped(dir: &Path, source: &str) -> Vec<String> {
    fs::read_to_string(tsa_path(dir, source))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| Some(format!("{}:{}", v["day"].as_str()?, v["hash"].as_str()?)))
        .collect()
}

fn retry_timestamps(dir: &Path, source: &str, records: &[Record], url: &str) {
    let done = stamped(dir, source);
    let chain = replay(records);
    for day in chain
        .days
        .iter()
        .filter(|d| !chain.expired.contains(&d.day))
    {
        if !done.contains(&format!("{}:{}", day.day, day.hash)) {
            let _ = request_timestamp(dir, source, &day.day, &day.hash, url);
        }
    }
}

/// Cancellazione su richiesta da giorni già sigillati: ricalcola l'impronta di ogni giorno
/// toccato e dei successivi (finché non si incontra un giorno già scaduto, la cui impronta
/// resta quella scritta) e aggiunge al registro una nota con il motivo.
/// `removed_by_day`: giorno (AAAA-MM-GG, UTC) → righe tolte quel giorno.
pub fn amend(
    dir: &Path,
    source: &Source,
    store: &Archive,
    removed_by_day: &std::collections::BTreeMap<String, i64>,
    reason: &str,
    now: i64,
) -> Result<usize> {
    let path = chain_path(dir, &source.id);
    let records = read_records(&path)?;
    let chain = replay(&records);
    let mut changes = Vec::new();
    let mut carry: Option<String> = None; // nuova impronta del giorno precedente, se cambiata
    for day in &chain.days {
        if chain.expired.contains(&day.day) {
            carry = None;
            continue;
        }
        if !removed_by_day.contains_key(&day.day) && carry.is_none() {
            continue;
        }
        let previous = carry.clone().unwrap_or_else(|| day.previous.clone());
        let (hash, count) = day_hash(store, day_from_name(&day.day)?, &previous)?;
        changes.push(Change {
            day: day.day.clone(),
            count,
            removed: removed_by_day.get(&day.day).copied().unwrap_or(0),
            old_hash: day.hash.clone(),
            new_hash: hash.clone(),
            previous,
        });
        carry = Some(hash);
    }
    if changes.is_empty() {
        return Ok(0);
    }
    let note = Record::Amended {
        at: crate::timestamps::format_ns(now),
        reason: reason.to_owned(),
        rows_removed: removed_by_day.values().sum(),
        changes: changes.clone(),
    };
    append(&path, &serde_json::to_string(&note)?)?;
    if let Some(url) = source.proof.as_ref().and_then(|p| p.tsa.as_ref()) {
        for change in &changes {
            let _ = request_timestamp(dir, &source.id, &change.day, &change.new_hash, url);
        }
    }
    Ok(changes.len())
}

fn hex(text: &str) -> Result<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(text.get(i..i + 2).unwrap_or("zz"), 16)
                .context("esadecimale non valido")
        })
        .collect()
}

// ---------- verifica ----------

/// Esito di una verifica: il rapporto giorno per giorno e quanti problemi ha trovato.
pub struct Verification {
    pub report: String,
    pub problems: usize,
}

/// Ricalcola ogni giorno ancora presente e controlla la catena. Funziona anche su una
/// copia: basta indicare l'archivio e il registro.
pub fn verify(archive: &Path, chain: &Path) -> Result<Verification> {
    let store = Archive::open(archive)?;
    let records = read_records(chain)?;
    if records.is_empty() {
        bail!(
            "Il registro della prova è vuoto o mancante: {}",
            chain.display()
        );
    }
    let chain = replay(&records);
    let mut report = Vec::new();
    let mut broken = 0;
    let mut prior: Option<&Day> = None;
    for day in &chain.days {
        let expired = chain.expired.contains(&day.day);
        // Un giorno scaduto non si può ricalcolare: se un giorno prima è stato modificato,
        // è lecito che punti ancora alla sua vecchia impronta (scritta nel registro).
        let linked = match prior {
            None => day.previous == "inizio",
            Some(p) => day.previous == p.hash || (expired && chain.retired.contains(&day.previous)),
        };
        if !linked {
            broken += 1;
            report.push(format!(
                "{}: CATENA INTERROTTA (manca o è cambiato un giorno precedente)",
                day.day
            ));
        }
        prior = Some(day);
        if expired {
            report.push(format!(
                "{}: scaduto e cancellato come previsto ({} righe)",
                day.day, day.count
            ));
        } else {
            let (actual, actual_count) = day_hash(&store, day_from_name(&day.day)?, &day.previous)?;
            if actual == day.hash {
                report.push(format!("{}: integro ({} righe)", day.day, day.count));
            } else {
                broken += 1;
                report.push(format!(
                    "{}: ALTERATO ({actual_count} righe ora, {} al sigillo)",
                    day.day, day.count
                ));
            }
        }
        for note in chain.notes.get(&day.day).into_iter().flatten() {
            let what = if note.rows_removed > 0 {
                format!("{} righe tolte da questo giorno", note.rows_removed)
            } else {
                "nessuna riga tolta: impronta ricalcolata perché un giorno prima è cambiato"
                    .to_owned()
            };
            report.push(format!(
                "    ↳ modificato il {}: {} ({what}; impronta {}… → {}…)",
                note.at,
                note.reason,
                &note.old_hash[..12],
                &note.new_hash[..12]
            ));
        }
    }
    report.push(String::new());
    report.push(
        "Un giorno integro non è cambiato dopo il sigillo; con la marca temporale, non dopo \
         l'ora della marca. Non è una prova che le righe fossero corrette prima."
            .into(),
    );
    report.push(if broken == 0 {
        "Verifica superata.".to_owned()
    } else {
        format!("Verifica NON superata: giorni con problemi: {broken}.")
    });
    Ok(Verification {
        report: report.join("\n"),
        problems: broken,
    })
}

/// Verifica la sorgente indicata con i file della cartella dati.
pub fn verify_source(dir: &Path, source: &str) -> Result<Verification> {
    verify(&config::archive_path(dir, source), &chain_path(dir, source))
}
