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
    /// Servizi systemd, es. `myapp.service`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    /// Identificatori syslog, es. `sshd` o `sudo`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers: Vec<String>,
    /// Campi del journal da confrontare, es. `CONTAINER_NAME=web` (un container Podman
    /// seguito per nome: continua anche quando viene ricreato).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<String>,
    /// Solo voci fino a questa gravità (0 = emergenza … 7 = debug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
}

impl Filter {
    pub fn validate(&self) -> Result<()> {
        if self.priority.is_some_and(|p| p > 7) {
            bail!("La gravità va da 0 (emergenza) a 7 (debug)");
        }
        for field in &self.matches {
            let valid = field.split_once('=').is_some_and(|(name, value)| {
                !name.is_empty()
                    && !value.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            });
            if !valid {
                bail!("Campo del journal non valido: {field:?} (formato NOME=valore)");
            }
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
        if !self.matches.is_empty() {
            parts.push(self.matches.join(", "));
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

    pub(crate) fn apply(&self, command: &mut Command) {
        for unit in &self.units {
            command.arg(format!("--unit={unit}"));
        }
        for identifier in &self.identifiers {
            command.arg(format!("--identifier={identifier}"));
        }
        if let Some(priority) = self.priority {
            command.arg(format!("--priority={priority}"));
        }
        // Le corrispondenze NOME=valore vanno per ultime, come argomenti semplici.
        command.args(&self.matches);
    }
}

/// Il programma da eseguire; `LOGLEM_JOURNALCTL` serve per le prove.
/// `journalctl` si può avviare su questo computer? Su macOS e dove manca systemd, no.
pub fn available() -> bool {
    journalctl()
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn journalctl() -> Command {
    Command::new(std::env::var_os("LOGLEM_JOURNALCTL").unwrap_or_else(|| "journalctl".into()))
}

pub fn history(filter: &Filter) -> Result<String> {
    filter.validate()?;
    let mut cmd = journalctl();
    cmd.args([
        "-n",
        "+1",
        "--no-pager",
        "--output=json",
        "--output-fields=__REALTIME_TIMESTAMP",
    ]);
    filter.apply(&mut cmd);
    let data = process::output(&mut cmd, Duration::from_secs(5))?;
    if data.is_empty() {
        return Ok("Nessuna voce leggibile per questa sorgente.".into());
    }
    let v: Value = serde_json::from_slice(&data)?;
    let at = v["__REALTIME_TIMESTAMP"]
        .as_str()
        .context("Data non disponibile")?
        .parse::<i64>()?
        .saturating_mul(1000);
    Ok(format!(
        "Prima voce leggibile: {} ({} giorni fa).",
        &crate::timestamps::format_ns(at)[..10],
        (crate::timestamps::now_ns() - at).max(0) / 86_400_000_000_000
    ))
}

/// Cursore dell'ultima voce letta: serve a riconoscere le lacune.
const CURSOR: &str = "journal_cursor";
/// Istante (microsecondi) dell'ultima voce letta: da qui riparte la lettura.
const SINCE: &str = "journal_since";
pub const WARNING: &str = "journal_warning";
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
    decode_value(&value, now)
}

fn decode_value(value: &Value, now: i64) -> Result<(String, Entry)> {
    let field = |name: &str| value.get(name).and_then(Value::as_str);
    let cursor = field("__CURSOR")
        .context("Voce del journal senza cursore")?
        .to_owned();
    let micros: i64 = field("__REALTIME_TIMESTAMP")
        .context("Voce del journal senza data")?
        .parse()
        .context("Data del journal non valida")?;
    // I container scrivono la riga con il suo a capo finale: non fa parte del messaggio.
    let text = message_text(value.get("MESSAGE"))
        .trim_end_matches(['\r', '\n'])
        .to_owned();
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

fn resume(
    store: &Archive,
    filter: &Filter,
    now: i64,
    earliest: i64,
    partial_view: bool,
) -> Result<i64> {
    let cursor = store.get_state(CURSOR)?.filter(|c| !c.is_empty());
    // Le lacune nascono quando Loglem resta spento: basta controllare una volta per avvio.
    // L'esito resta salvato fino al controllo successivo.
    let this_run = std::process::id().to_string();
    let checked = store.get_state(GAP_CHECKED)?.as_deref() == Some(this_run.as_str());
    // Se non si vede tutto il journal, una voce che manca non è per forza stata eliminata.

    if let (Some(cursor), false) = (&cursor, checked) {
        if !partial_view && !cursor_still_available(cursor, filter)? {
            let from = cursor_time(cursor).unwrap_or(0).saturating_mul(1000);
            store.record_gap(
                from,
                now,
                "Il journal ha eliminato voci prima che Loglem le leggesse",
            )?;
        }
        store.set_state(GAP_CHECKED, &this_run)?;
    }

    // Si riparte dall'istante dell'ultima voce letta, non dal cursore: se il journal
    // ha eliminato quella voce, `--after-cursor` non restituirebbe più nulla.
    // Le voci di quel secondo vengono rilette e scartate dall'archivio (stessa chiave).
    let since_seconds = store
        .get_state(SINCE)?
        .and_then(|s| s.parse::<i64>().ok())
        .map(|micros| micros / 1_000_000)
        .unwrap_or(0)
        .max(earliest.max(0) / 1_000_000_000);
    Ok(since_seconds)
}

pub fn collect(store: &mut Archive, filter: &Filter, now: i64, earliest: i64) -> Result<usize> {
    let partial_view = sees_only_own_entries();
    let since_seconds = resume(store, filter, now, earliest, partial_view)?;
    let mut warnings = Vec::new();
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

    if diagnostic.contains("insufficient permissions")
        || diagnostic.contains("other users")
        || partial_view
    {
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

/// Su Linux, chi non è root e non è in uno dei gruppi che leggono tutto il journal vede solo
/// le proprie voci. `journalctl` lo dice solo a volte, quindi lo controlliamo noi.
fn sees_only_own_entries() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let ask = |option: &str| {
        process::output(Command::new("id").arg(option), Duration::from_secs(5))
            .ok()
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
    };
    let (Some(uid), Some(groups)) = (ask("-u"), ask("-nG")) else {
        return false;
    };
    uid != "0"
        && !groups
            .split_whitespace()
            .any(|g| matches!(g, "systemd-journal" | "adm" | "wheel"))
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

fn selected(f: &Filter, row: &Value) -> bool {
    let has = |key: &str, value: &str| {
        row[key].as_str() == Some(value)
            || row[key]
                .as_array()
                .is_some_and(|vs| vs.iter().any(|v| v.as_str() == Some(value)))
    };
    let units = f.units.is_empty()
        || f.units.iter().any(|unit| {
            let unit = if unit.contains('.') {
                unit.clone()
            } else {
                format!("{unit}.service")
            };
            let pattern = glob::Pattern::new(&unit).ok();
            let matched = |key: &str| {
                row[key]
                    .as_str()
                    .is_some_and(|v| pattern.as_ref().is_some_and(|p| p.matches(v)))
            };
            matched("_SYSTEMD_UNIT")
                || (has("_PID", "1") && matched("UNIT"))
                || (has("_UID", "0")
                    && (matched("OBJECT_SYSTEMD_UNIT") || matched("COREDUMP_UNIT")))
        });
    units
        && (f.identifiers.is_empty() || f.identifiers.iter().any(|id| has("SYSLOG_IDENTIFIER", id)))
        && f.priority.is_none_or(|p| {
            row["PRIORITY"]
                .as_str()
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|v| v <= p)
        })
        && f.matches
            .iter()
            .filter_map(|m| m.split_once('='))
            .all(|(key, _)| {
                f.matches
                    .iter()
                    .filter_map(|m| m.split_once('='))
                    .any(|(k, v)| k == key && has(k, v))
            })
}

struct Subscriber<'a> {
    id: &'a str,
    filter: &'a Filter,
    path: std::path::PathBuf,
    since: i64,
    earliest: i64,
    batch: Vec<Entry>,
    last: Option<(String, i64)>,
    added: usize,
}
impl Subscriber<'_> {
    fn flush(&mut self) -> Result<()> {
        if self.last.is_some() {
            let mut db = Archive::open(&self.path)?;
            self.added += save(&mut db, &mut self.batch, self.last.as_ref())?;
            self.last = None;
        }
        Ok(())
    }
}

pub(crate) fn collect_shared(
    dir: &std::path::Path,
    sources: &[crate::model::Source],
) -> std::collections::HashMap<String, Result<usize>> {
    let mut results = std::collections::HashMap::new();
    let mut subscriptions = Vec::new();
    let now = crate::timestamps::now_ns();
    let journals: Vec<_> = sources
        .iter()
        .filter_map(|s| match &s.input {
            crate::model::Input::Journal(f) if s.enabled && !s.detached => Some((s, f)),
            _ => None,
        })
        .collect();
    if journals.len() < 2 {
        return results;
    }
    let partial = sees_only_own_entries();
    for (s, f) in journals {
        let path = s.archive_path(dir);
        let earliest = s.retention.cutoff(now);
        match Archive::open(&path).and_then(|db| resume(&db, f, now, earliest, partial)) {
            Ok(since) => subscriptions.push(Subscriber {
                id: &s.id,
                filter: f,
                path,
                since,
                earliest,
                batch: vec![],
                last: None,
                added: 0,
            }),
            Err(e) => {
                results.insert(s.id.clone(), Err(e));
            }
        }
    }
    let Some(since) = subscriptions.iter().map(|s| s.since).min() else {
        return results;
    };
    let mut command = journalctl();
    command
        .args(["--output=json", "--no-pager"])
        .arg(format!("--since=@{since}"));
    // Narrow the union with a necessary field for each subscription when possible.
    if subscriptions
        .iter()
        .all(|s| !s.filter.matches.is_empty() || !s.filter.identifiers.is_empty())
    {
        for (i, s) in subscriptions.iter().enumerate() {
            if i > 0 {
                command.arg("+");
            }
            if !s.filter.matches.is_empty() {
                command.args(&s.filter.matches);
            } else {
                command.args(
                    s.filter
                        .identifiers
                        .iter()
                        .map(|v| format!("SYSLOG_IDENTIFIER={v}")),
                );
            }
        }
    }
    let mut used = 0;
    let mut count = 0;
    let mut warning = partial;
    let result = process::stream(&mut command, Duration::from_secs(120), |stderr, line| {
        if stderr {
            let msg = String::from_utf8_lossy(&line);
            warning |= msg.contains("insufficient permissions") || msg.contains("other users");
            return Ok(());
        }
        let row: Value = serde_json::from_slice(&line)?;
        let (cursor, entry) = decode_value(&row, now)?;
        for sub in &mut subscriptions {
            if entry.event_ns < sub.earliest
                || entry.event_ns / 1_000_000_000 < sub.since
                || !selected(sub.filter, &row)
            {
                continue;
            }
            used += entry.message.len();
            count += 1;
            sub.batch.push(entry.clone());
            sub.last = Some((cursor.clone(), entry.event_ns / 1000));
            if used >= 1024 * 1024 || count >= BATCH_SIZE {
                sub.flush()?;
            }
        }
        if used >= 1024 * 1024 || count >= BATCH_SIZE {
            for s in &mut subscriptions {
                s.flush()?;
            }
            used = 0;
            count = 0;
        }
        Ok(())
    })
    .and_then(|s| {
        anyhow::ensure!(s.success(), "Lettura del journal interrotta");
        Ok(())
    });
    for mut sub in subscriptions {
        let value=sub.flush().and_then(|_|{
            if let Err(e)=&result {bail!("{e:#}");}
            Archive::open(&sub.path)?.set_state(WARNING,if warning {"Loglem vede solo una parte del journal. Controlla i permessi del gruppo systemd-journal."} else {""})?;
            Ok(sub.added)
        });
        results.insert(sub.id.to_owned(), value);
    }
    results
}
