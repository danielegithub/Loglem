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
    /// Il controllo di salute che Loglem fa da solo, a intervalli.
    #[serde(default, skip_serializing_if = "crate::monitor::Settings::is_default")]
    pub monitor: crate::monitor::Settings,
    /// Avvisi per mail (facoltativi). La chiave del servizio non sta qui.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<crate::email::Settings>,
}

impl Alerts {
    pub fn is_default(&self) -> bool {
        self.rules.is_empty()
            && self.command.is_none()
            && self.monitor.is_default()
            && self.email.is_none()
    }

    pub fn validate(&self) -> Result<()> {
        self.monitor.validate()?;
        if let Some(email) = &self.email {
            email.validate()?;
        }
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
                let since = now.saturating_sub(i64::from(*minutes).saturating_mul(60_000_000_000));
                let mut found = 0;
                for s in config.sources.iter().filter(|s| in_scope(&s.id, source)) {
                    let path = config::archive_path(dir, &s.id);
                    if path.exists() {
                        found += Archive::open(&path)?.pattern_hits(since, text, *minutes)?;
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

pub(crate) fn visible(dir: &Path, problems: Vec<Problem>) -> Vec<Problem> {
    let seen = acks(dir);
    problems
        .into_iter()
        .filter(|p| !seen.contains_key(&p.id))
        .collect()
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
    /// Tutti i problemi aperti, anche quelli «segnati come visti»: serve a sapere da quando
    /// durano e a distinguere un problema risolto da uno solo nascosto.
    #[serde(default)]
    open: BTreeMap<String, Open>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub since: String,
    pub text: String,
}

/// Cosa è cambiato tra un controllo e il precedente. Serve agli avvisi per mail.
#[derive(Debug, Clone, Default)]
pub struct Change {
    /// Problemi comparsi ora e non ancora «visti».
    pub opened: Vec<Problem>,
    /// Problemi scomparsi: sono davvero risolti.
    pub closed: Vec<Problem>,
    /// Problemi non visti aperti da più di 24 ore, con le ore trascorse.
    pub open_long: Vec<(Problem, i64)>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.opened.is_empty() && self.closed.is_empty() && self.open_long.is_empty()
    }
}

const DAY_NS: i64 = 86_400_000_000_000;

fn parse_ns(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()?
        .timestamp_nanos_opt()
}

/// Confronta i problemi di adesso con quelli del controllo precedente.
/// `all` = tutti i problemi; `visible` = quelli non «segnati come visti».
fn transitions(
    previous: &BTreeMap<String, Open>,
    all: &[Problem],
    visible: &[Problem],
    now: i64,
) -> (BTreeMap<String, Open>, Change) {
    let mut open = BTreeMap::new();
    let mut change = Change::default();
    for problem in all {
        let known = previous.get(&problem.id);
        open.insert(
            problem.id.clone(),
            Open {
                since: known.map_or_else(|| crate::timestamps::format_ns(now), |k| k.since.clone()),
                text: problem.text.clone(),
            },
        );
    }
    for problem in visible {
        match previous.get(&problem.id) {
            None => change.opened.push(problem.clone()),
            Some(known) => {
                let age = now.saturating_sub(parse_ns(&known.since).unwrap_or(now));
                if age >= DAY_NS {
                    change
                        .open_long
                        .push((problem.clone(), age / 3_600_000_000_000));
                }
            }
        }
    }
    for (id, known) in previous {
        if !open.contains_key(id) {
            change.closed.push(Problem {
                id: id.clone(),
                source: None,
                text: known.text.clone(),
            });
        }
    }
    (open, change)
}

/// Quando e con che esito è stato fatto l'ultimo controllo: (data, problemi aperti).
pub fn last_check(dir: &Path) -> Option<(String, usize)> {
    let file: HealthFile = serde_json::from_slice(&fs::read(dir.join(STATE)).ok()?).ok()?;
    let count = file.problems.len();
    (!file.checked_at.is_empty()).then_some((file.checked_at, count))
}

pub(crate) fn cached_rules(dir: &Path) -> Result<Vec<Problem>> {
    match fs::read(dir.join(STATE)) {
        Ok(bytes) => Ok(serde_json::from_slice::<HealthFile>(&bytes)?
            .problems
            .into_iter()
            .filter(|p| p.id.starts_with("regola:"))
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

/// Dopo ogni giro di raccolta: aggiorna `health.json` (leggibile da altri programmi) e,
/// se l'elenco dei problemi è cambiato, esegue il comando di notifica configurato.
pub fn after_cycle(dir: &Path, problems: Vec<Problem>) -> Result<Change> {
    let all = problems.clone();
    let problems = hide_acknowledged(dir, problems);
    let previous: HealthFile = fs::read(dir.join(STATE))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default();
    let ids = |list: &[Problem]| list.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let changed = ids(&previous.problems) != ids(&problems);
    let now = crate::timestamps::now_ns();
    let (open, change) = transitions(&previous.open, &all, &problems, now);
    let state = HealthFile {
        checked_at: crate::timestamps::format_ns(now),
        ok: problems.is_empty(),
        problems,
        open,
    };
    util::atomic_write(&dir.join(STATE), &serde_json::to_vec_pretty(&state)?)?;
    if changed {
        if let Some(command) = config::load(dir)?.alerts.command {
            notify(&command, &state.problems);
        }
    }
    Ok(change)
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

#[cfg(test)]
mod transition_tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000_000_000;
    const HOUR: i64 = 3_600_000_000_000;

    fn problem(id: &str) -> Problem {
        Problem {
            id: id.into(),
            source: None,
            text: format!("testo di {id}"),
        }
    }

    fn open(id: &str, hours_ago: i64) -> (String, Open) {
        (
            id.to_owned(),
            Open {
                since: crate::timestamps::format_ns(NOW - hours_ago * HOUR),
                text: format!("testo di {id}"),
            },
        )
    }

    #[test]
    fn new_resolved_and_long_open_problems_are_told_apart() {
        let previous: BTreeMap<_, _> =
            [open("vecchio", 30), open("recente", 2), open("risolto", 5)].into();
        let all = vec![problem("vecchio"), problem("recente"), problem("nuovo")];
        let (now_open, change) = transitions(&previous, &all, &all, NOW);
        assert_eq!(
            change
                .opened
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["nuovo"]
        );
        assert_eq!(
            change
                .closed
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["risolto"]
        );
        assert_eq!(change.open_long.len(), 1);
        assert_eq!(change.open_long[0].0.id, "vecchio");
        assert_eq!(change.open_long[0].1, 30);
        // «da quando» si conserva: il vecchio resta vecchio, il nuovo parte da adesso.
        assert_eq!(now_open["vecchio"].since, previous["vecchio"].since);
        assert_eq!(now_open["nuovo"].since, crate::timestamps::format_ns(NOW));
    }

    #[test]
    fn a_problem_marked_as_seen_is_neither_new_nor_reminded_but_not_resolved_either() {
        let previous: BTreeMap<_, _> = [open("visto", 30)].into();
        let all = vec![problem("visto"), problem("nuovo-visto")];
        // L'utente ha già «segnato come visto» entrambi: non sono tra i visibili.
        let (now_open, change) = transitions(&previous, &all, &[], NOW);
        assert!(
            change.is_empty(),
            "niente mail per quello che l'utente ha già visto"
        );
        assert!(now_open.contains_key("visto"), "il problema esiste ancora");
        // Quando sparisce davvero, si dice che è risolto.
        let (_, change) = transitions(&now_open, &[], &[], NOW + HOUR);
        assert_eq!(change.closed.len(), 2);
    }
}
