//! Allarmi: regole dell'utente, «segna come visto», file di stato e comando di notifica.
//!
//! Ogni problema ha un nome stabile (es. `ferma:web`, `regola:accessi-falliti`): serve a
//! segnarlo come visto e a capire, tra un controllo e l'altro, se qualcosa è cambiato.
use crate::{archive::Archive, config, health::Problem, util};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::Duration};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alerts {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Eseguito quando l'elenco dei problemi cambia; riceve il riepilogo in `LOGLEM_ALERTS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl Alerts {
    pub fn is_default(&self) -> bool {
        self.rules.is_empty() && self.command.is_none()
    }

    pub fn validate(&self) -> Result<()> {
        let mut names = std::collections::HashSet::new();
        for rule in &self.rules {
            crate::config::validate_id(&rule.name)?;
            if !names.insert(&rule.name) {
                bail!("Regola {} ripetuta", rule.name);
            }
            if let Check::Pattern {
                text,
                count,
                minutes,
                ..
            } = &rule.check
            {
                if text.is_empty() || *count == 0 || *minutes == 0 {
                    bail!(
                        "La regola {} ha bisogno di testo, numero e minuti",
                        rule.name
                    );
                }
            }
        }
        Ok(())
    }
}

// Niente `deny_unknown_fields` qui: serde non lo permette insieme a `flatten`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(flatten)]
    pub check: Check,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    /// Un testo compare almeno `count` volte negli ultimi `minutes` minuti.
    Pattern {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        text: String,
        count: u32,
        minutes: u32,
    },
    /// L'archivio (di una sorgente o di tutte) supera questa dimensione.
    ArchiveSize {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        megabytes: u64,
    },
    /// Sul disco della cartella dati resta meno di questo spazio.
    FreeSpace { megabytes: u64 },
}

impl Rule {
    pub fn describe(&self) -> String {
        match &self.check {
            Check::Pattern {
                source,
                text,
                count,
                minutes,
            } => format!(
                "«{text}» almeno {count} volte in {minutes} minuti{}",
                source
                    .as_deref()
                    .map(|s| format!(" in {s}"))
                    .unwrap_or_default()
            ),
            Check::ArchiveSize { source, megabytes } => format!(
                "archivio{} oltre {megabytes} MB",
                source
                    .as_deref()
                    .map(|s| format!(" di {s}"))
                    .unwrap_or_default()
            ),
            Check::FreeSpace { megabytes } => format!("meno di {megabytes} MB liberi sul disco"),
        }
    }
}

/// Le regole dell'utente che in questo momento scattano.
pub fn evaluate(dir: &Path) -> Result<Vec<Problem>> {
    let config = config::load(dir)?;
    let now = crate::timestamps::now_ns();
    let mut problems = Vec::new();
    for rule in &config.alerts.rules {
        let in_scope =
            |id: &str, wanted: &Option<String>| wanted.as_deref().is_none_or(|w| w == id);
        let fired = match &rule.check {
            Check::Pattern {
                source,
                text,
                count,
                minutes,
            } => {
                let since = now - i64::from(*minutes) * 60_000_000_000;
                let mut found = 0;
                for s in config.sources.iter().filter(|s| in_scope(&s.id, source)) {
                    let path = config::archive_path(dir, &s.id);
                    if path.exists() {
                        found += Archive::open(&path)?.count_since(since, text)?;
                    }
                }
                (found >= i64::from(*count))
                    .then(|| format!("{found} volte negli ultimi {minutes} minuti"))
            }
            Check::ArchiveSize { source, megabytes } => {
                let bytes: u64 = config
                    .sources
                    .iter()
                    .filter(|s| in_scope(&s.id, source))
                    .map(|s| util::bytes_on_disk(&config::archive_path(dir, &s.id)))
                    .sum();
                (bytes > megabytes * 1024 * 1024).then(|| format!("{} MB", bytes / 1024 / 1024))
            }
            Check::FreeSpace { megabytes } => {
                let free = fs2::available_space(dir)?;
                (free < megabytes * 1024 * 1024)
                    .then(|| format!("{} MB liberi", free / 1024 / 1024))
            }
        };
        if let Some(detail) = fired {
            problems.push(Problem {
                id: format!("regola:{}", rule.name),
                source: None,
                text: format!("Regola {}: {} ({detail})", rule.name, rule.describe()),
            });
        }
    }
    Ok(problems)
}

// ---------- «segna come visto» ----------

const ACKS: &str = "alerts-ack.json";

fn acks(dir: &Path) -> BTreeMap<String, String> {
    fs::read(dir.join(ACKS))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

/// Segna un problema come visto: non viene più mostrato finché non sparisce e ricompare.
pub fn acknowledge(dir: &Path, id: &str) -> Result<()> {
    let mut seen = acks(dir);
    seen.insert(
        id.to_owned(),
        crate::timestamps::format_ns(crate::timestamps::now_ns()),
    );
    util::atomic_write(&dir.join(ACKS), &serde_json::to_vec_pretty(&seen)?)
}

/// Toglie i problemi già visti; dimentica quelli risolti, così se ricompaiono si rivedono.
pub fn hide_acknowledged(dir: &Path, problems: Vec<Problem>) -> Vec<Problem> {
    let mut seen = acks(dir);
    let before = seen.len();
    seen.retain(|id, _| problems.iter().any(|p| &p.id == id));
    if seen.len() != before {
        let _ =
            serde_json::to_vec_pretty(&seen).map(|data| util::atomic_write(&dir.join(ACKS), &data));
    }
    problems
        .into_iter()
        .filter(|p| !seen.contains_key(&p.id))
        .collect()
}

// ---------- file di stato e notifica ----------

const STATE: &str = "health.json";

#[derive(Serialize, Deserialize, Default)]
struct HealthFile {
    checked_at: String,
    ok: bool,
    problems: Vec<Problem>,
}

/// Dopo ogni giro di raccolta: aggiorna `health.json` (leggibile da altri programmi) e,
/// se l'elenco dei problemi è cambiato, esegue il comando di notifica configurato.
pub fn after_cycle(dir: &Path, problems: Vec<Problem>) -> Result<()> {
    let previous: HealthFile = fs::read(dir.join(STATE))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default();
    let ids = |list: &[Problem]| list.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let changed = ids(&previous.problems) != ids(&problems);
    let state = HealthFile {
        checked_at: crate::timestamps::format_ns(crate::timestamps::now_ns()),
        ok: problems.is_empty(),
        problems,
    };
    util::atomic_write(&dir.join(STATE), &serde_json::to_vec_pretty(&state)?)?;
    if changed {
        if let Some(command) = config::load(dir)?.alerts.command {
            notify(&command, &state.problems);
        }
    }
    Ok(())
}

/// Esegue il comando dell'utente senza bloccare la raccolta; il testo arriva in variabili.
fn notify(command: &str, problems: &[Problem]) {
    let summary = if problems.is_empty() {
        "Loglem: tutto a posto".to_owned()
    } else {
        problems
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    #[cfg(unix)]
    let mut process = Command::new("sh");
    #[cfg(unix)]
    process.arg("-c");
    #[cfg(windows)]
    let mut process = Command::new("cmd");
    #[cfg(windows)]
    process.arg("/C");
    process
        .arg(command)
        .env("LOGLEM_ALERTS", summary)
        .env("LOGLEM_ALERT_COUNT", problems.len().to_string());
    std::thread::spawn(move || {
        let result = crate::process::output(&mut process, Duration::from_secs(60));
        if let Err(error) = result {
            crate::diagnostics::error("alerts", "notify", None, &error);
        }
    });
}
