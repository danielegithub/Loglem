//! Journal di systemd, letto con `journalctl --output=json`.
//!
//! Dopo un riavvio Loglem riprende dall'istante dell'ultima voce letta. Se nel
//! frattempo il journal ha eliminato voci non ancora lette, la scheda lo segnala.
use crate::{archive::Archive, model::Entry, process, util, BATCH_SIZE};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{process::Command, time::Duration};

/// Quali voci del journal leggere. Liste vuote = nessun filtro.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    /// Servizi systemd, es. `caddy.service`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    /// Identificatori syslog, es. `sshd` o `sudo`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers: Vec<String>,
    /// Solo voci fino a questa gravità (0 = emergenza … 7 = debug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
}

impl Filter {
    pub fn validate(&self) -> Result<()> {
        if self.priority.is_some_and(|p| p > 7) {
            bail!("La gravità va da 0 (emergenza) a 7 (debug)");
        }
        for name in self.units.iter().chain(&self.identifiers) {
            if name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
                bail!("Nome di servizio o identificatore non valido: {name:?}");
            }
        }
        Ok(())
    }

    /// Descrizione leggibile, per le schede e il terminale.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.units.is_empty() {
            parts.push(format!("servizi {}", self.units.join(", ")));
        }
        if !self.identifiers.is_empty() {
            parts.push(format!("programmi {}", self.identifiers.join(", ")));
        }
        if let Some(p) = self.priority {
            parts.push(format!("gravità fino a {p}"));
        }
        if parts.is_empty() {
            "tutto il journal".into()
        } else {
            parts.join(" · ")
        }
    }

    fn apply(&self, command: &mut Command) {
        for unit in &self.units {
            command.arg(format!("--unit={unit}"));
        }
        for identifier in &self.identifiers {
            command.arg(format!("--identifier={identifier}"));
        }
        if let Some(priority) = self.priority {
            command.arg(format!("--priority={priority}"));
        }
    }
}

/// Il programma da eseguire; `LOGLEM_JOURNALCTL` serve per le prove.
fn journalctl() -> Command {
    Command::new(std::env::var_os("LOGLEM_JOURNALCTL").unwrap_or_else(|| "journalctl".into()))
}

/// Cursore dell'ultima voce letta: serve a riconoscere le lacune.
const CURSOR: &str = "journal_cursor";
/// Istante (microsecondi) dell'ultima voce letta: da qui riparte la lettura.
const SINCE: &str = "journal_since";
pub const WARNING: &str = "journal_warning";
const GAP: &str = "journal_gap";
const GAP_CHECKED: &str = "journal_gap_checked";

/// Servizi presenti nel journal, per proporli nella scelta guidata.
pub fn units() -> Result<Vec<String>> {
    let data = process::output(
        journalctl().args(["--field=_SYSTEMD_UNIT", "--no-pager"]),
        Duration::from_secs(20),
    )
    .context("Non riesco a leggere il journal: journalctl è installato?")?;
    let mut units: Vec<String> = String::from_utf8_lossy(&data)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    units.sort();
    Ok(units)
}

/// Trasforma una riga JSON di journalctl in una voce dell'archivio.
fn decode(line: &[u8], now: i64) -> Result<(String, Entry)> {
    let value: Value = serde_json::from_slice(line).context("journalctl non ha restituito JSON")?;
    let field = |name: &str| value.get(name).and_then(Value::as_str);
    let cursor = field("__CURSOR")
        .context("Voce del journal senza cursore")?
        .to_owned();
    let micros: i64 = field("__REALTIME_TIMESTAMP")
        .context("Voce del journal senza data")?
        .parse()
        .context("Data del journal non valida")?;
    let text = message_text(value.get("MESSAGE"));
    let source = field("SYSLOG_IDENTIFIER")
        .or_else(|| field("_COMM"))
        .or_else(|| field("_SYSTEMD_UNIT"))
        .unwrap_or("journal");
    let message = match field("_PID") {
        Some(pid) => format!("{source}[{pid}]: {text}"),
        None => format!("{source}: {text}"),
    };
    let entry = Entry {
        key: format!("journal:{}", util::digest(cursor.as_bytes())),
        file_id: None,
        generation: 0,
        offset: 0,
        event_ns: micros.saturating_mul(1000),
        collected_ns: now,
        timestamp_origin: "event",
        raw: line.to_vec(),
        message,
    };
    Ok((cursor, entry))
}

/// `MESSAGE` può essere testo, un elenco di byte (testo non UTF-8) o mancare.
fn message_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(bytes)) => {
            let bytes: Vec<u8> = bytes
                .iter()
                .filter_map(|b| b.as_u64())
                .map(|b| b as u8)
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
        _ => String::new(),
    }
}

pub fn collect(store: &mut Archive, filter: &Filter, now: i64, earliest: i64) -> Result<usize> {
    let cursor = store.get_state(CURSOR)?.filter(|c| !c.is_empty());
    // Le lacune nascono quando Loglem resta spento: basta controllare una volta per avvio.
    // L'esito resta salvato fino al controllo successivo.
    let this_run = std::process::id().to_string();
    let checked = store.get_state(GAP_CHECKED)?.as_deref() == Some(this_run.as_str());
    if let (Some(cursor), false) = (&cursor, checked) {
        let gap = if cursor_still_available(cursor, filter)? {
            ""
        } else {
            "Il journal ha eliminato voci prima che Loglem le leggesse: \
             nello storico può mancare un periodo"
        };
        store.set_state(GAP, gap)?;
        store.set_state(GAP_CHECKED, &this_run)?;
    }
    let mut warnings: Vec<String> = store
        .get_state(GAP)?
        .into_iter()
        .filter(|g| !g.is_empty())
        .collect();

    // Si riparte dall'istante dell'ultima voce letta, non dal cursore: se il journal
    // ha eliminato quella voce, `--after-cursor` non restituirebbe più nulla.
    // Le voci di quel secondo vengono rilette e scartate dall'archivio (stessa chiave).
    let since_seconds = store
        .get_state(SINCE)?
        .and_then(|s| s.parse::<i64>().ok())
        .map(|micros| micros / 1_000_000)
        .unwrap_or(0)
        .max(earliest.max(0) / 1_000_000_000);
    let mut command = journalctl();
    command.args(["--output=json", "--no-pager"]);
    command.arg(format!("--since=@{since_seconds}"));
    filter.apply(&mut command);

    let mut batch = Vec::new();
    let mut last: Option<(String, i64)> = None;
    let mut added = 0;
    let mut diagnostic = String::new();
    let status = process::stream(&mut command, Duration::from_secs(120), |stderr, line| {
        if stderr {
            if diagnostic.len() < 4096 {
                diagnostic.push_str(String::from_utf8_lossy(&line).trim());
                diagnostic.push(' ');
            }
            return Ok(());
        }
        let (cursor, entry) = decode(&line, now)?;
        last = Some((cursor, entry.event_ns / 1000));
        if entry.event_ns >= earliest {
            batch.push(entry);
        }
        if batch.len() >= BATCH_SIZE {
            added += save(store, &mut batch, last.as_ref())?;
        }
        Ok(())
    })?;
    added += save(store, &mut batch, last.as_ref())?;

    if diagnostic.contains("insufficient permissions") || diagnostic.contains("other users") {
        warnings.push(
            "Loglem vede solo una parte del journal. Per leggerlo tutto, un amministratore \
             deve aggiungere l'utente al gruppo systemd-journal \
             (sudo usermod -aG systemd-journal UTENTE)"
                .to_owned(),
        );
    }
    store.set_state(WARNING, &warnings.join(". "))?;
    if !status.success() {
        bail!(
            "journalctl non ha completato la lettura: {}",
            util::safe_text(&diagnostic)
        );
    }
    Ok(added)
}

/// Salva le voci, poi il punto di ripartenza: dopo un'interruzione si riparte da qui.
fn save(
    store: &mut Archive,
    batch: &mut Vec<Entry>,
    last: Option<&(String, i64)>,
) -> Result<usize> {
    let added = store.save_entries(batch)?;
    batch.clear();
    if let Some((cursor, micros)) = last {
        store.set_state(CURSOR, cursor)?;
        store.set_state(SINCE, &micros.to_string())?;
    }
    Ok(added)
}

/// Vero se la voce del cursore salvato esiste ancora nel journal.
fn cursor_still_available(cursor: &str, filter: &Filter) -> Result<bool> {
    let mut command = journalctl();
    command.args(["--output=json", "--no-pager", "--lines=1", "--reverse"]);
    command.arg(format!(
        "--until=@{}",
        cursor_time(cursor).unwrap_or(0) / 1_000_000 + 1
    ));
    filter.apply(&mut command);
    // La voce più recente fino all'istante del cursore deve essere proprio quella.
    let mut found = None;
    process::stream(&mut command, Duration::from_secs(20), |stderr, line| {
        if !stderr && found.is_none() {
            found = decode(&line, 0).ok().map(|(c, _)| c);
        }
        Ok(())
    })?;
    Ok(found.is_some_and(|found| cursor_time(&found) >= cursor_time(cursor)))
}

/// L'istante (in microsecondi) scritto nel cursore, campo `t=`.
fn cursor_time(cursor: &str) -> Option<i64> {
    cursor
        .split(';')
        .find_map(|part| part.strip_prefix("t="))
        .and_then(|hex| i64::from_str_radix(hex, 16).ok())
}
