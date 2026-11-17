//! Registro degli accessi ai log: chi ha cercato, esportato, cancellato o cambiato le
//! impostazioni, e quando. Serve a scoprire un uso improprio (GDPR, art. 32).
//!
//! Non contiene i valori cercati: un IP o un'email scritti in una ricerca sarebbero a loro
//! volta dati personali. Si scrive solo il *tipo* di filtro («testo, ip»), non cosa c'era scritto.
//! Un file al mese in `audit/`, conservato 13 mesi.
use anyhow::Result;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const KEEP_MONTHS: i64 = 13;

fn month_of(timestamp: &str) -> &str {
    timestamp.get(..7).unwrap_or("0000-00")
}

/// Scrive una riga nel registro. Non fa mai fallire chi la chiama: un errore qui finisce
/// nella diagnostica.
pub fn record(dir: &Path, user: &str, action: &str, detail: &str) {
    if let Err(error) = try_record(dir, user, action, detail) {
        crate::diagnostics::error("audit", "record", None, &error);
    }
}

fn try_record(dir: &Path, user: &str, action: &str, detail: &str) -> Result<()> {
    let folder = dir.join("audit");
    crate::util::private_dir(&folder)?;
    let at = crate::timestamps::format_ns(crate::timestamps::now_ns());
    let line = json!({"at": at, "user": user, "action": action, "detail": detail});
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(folder.join(format!("{}.log", month_of(&at))))?;
    writeln!(file, "{line}")?;
    prune(&folder, month_of(&at));
    Ok(())
}

/// Toglie i file più vecchi di 13 mesi.
fn prune(folder: &Path, current: &str) {
    let number = |month: &str| -> Option<i64> {
        let (year, month) = month.split_once('-')?;
        Some(year.parse::<i64>().ok()? * 12 + month.parse::<i64>().ok()?)
    };
    let Some(now) = number(current) else { return };
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(month) = name.strip_suffix(".log").and_then(number) {
            if now - month > KEEP_MONTHS {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// Le voci degli ultimi `days` giorni, dalla più recente.
pub fn recent(dir: &Path, days: u32) -> Result<Vec<Value>> {
    let since = crate::timestamps::format_ns(
        crate::timestamps::now_ns() - i64::from(days) * 86_400 * 1_000_000_000,
    );
    let mut files: Vec<_> = fs::read_dir(dir.join("audit"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    let mut entries = Vec::new();
    for file in files {
        for line in fs::read_to_string(file)?.lines() {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                if entry["at"].as_str().is_some_and(|at| at >= since.as_str()) {
                    entries.push(entry);
                }
            }
        }
    }
    entries.reverse();
    Ok(entries)
}

/// Quali filtri sono stati usati, senza i valori: «testo, ip».
pub fn filter_kinds(query: &crate::query::Query) -> String {
    let mut kinds = Vec::new();
    if !query.text.is_empty() {
        kinds.push("testo");
    }
    if query.ip.as_deref().is_some_and(|s| !s.is_empty()) {
        kinds.push("ip");
    }
    if query.path.as_deref().is_some_and(|s| !s.is_empty()) {
        kinds.push("percorso");
    }
    if query.status.is_some() {
        kinds.push("codice");
    }
    if query.level.is_some() {
        kinds.push("gravità");
    }
    if kinds.is_empty() {
        "nessun filtro".into()
    } else {
        kinds.join(", ")
    }
}
