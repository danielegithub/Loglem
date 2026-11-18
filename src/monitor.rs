//! Controllo automatico: Loglem controlla da solo la propria salute, a intervalli regolari,
//! senza che l'utente debba collegarlo a cron. È lo stesso controllo di `loglem check`.
use crate::{alerts, config, health};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};

pub const MIN_MINUTES: u32 = 1;
pub const MAX_MINUTES: u32 = 1440;
const MINUTE_NS: i64 = 60_000_000_000;

/// Un solo controllo alla volta: lo chiedono sia il ciclo di lettura sia il pulsante.
static RUN: Mutex<()> = Mutex::new(());

fn yes() -> bool {
    true
}
fn five() -> u32 {
    5
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "five")]
    pub minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            minutes: 5,
        }
    }
}

impl Settings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (MIN_MINUTES..=MAX_MINUTES).contains(&self.minutes),
            "L'intervallo del controllo va da {MIN_MINUTES} minuto a {MAX_MINUTES} minuti (un giorno)"
        );
        Ok(())
    }
}

/// Lo stato che la pagina e il terminale mostrano.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Status {
    pub enabled: bool,
    pub minutes: u32,
    pub checked_at: Option<String>,
    pub age_minutes: Option<i64>,
    pub open: usize,
    /// `off`, `never`, `ok`, `problems` o `stale` (il controllo non gira da troppo tempo).
    pub state: &'static str,
}

fn classify(settings: Settings, last: Option<(String, usize)>, now: i64) -> Status {
    let age = last.as_ref().and_then(|(at, _)| {
        chrono::DateTime::parse_from_rfc3339(at)
            .ok()?
            .timestamp_nanos_opt()
            .map(|then| now.saturating_sub(then) / MINUTE_NS)
    });
    let open = last.as_ref().map_or(0, |(_, n)| *n);
    let state = if !settings.enabled {
        "off"
    } else if age.is_none() {
        "never"
    } else if age.is_some_and(|a| a > 2 * i64::from(settings.minutes) + 1) {
        // Ha tempo di girare due volte: se non l'ha fatto, qualcosa non va.
        "stale"
    } else if open > 0 {
        "problems"
    } else {
        "ok"
    };
    Status {
        enabled: settings.enabled,
        minutes: settings.minutes,
        checked_at: last.map(|(at, _)| at),
        age_minutes: age,
        open,
        state,
    }
}

pub fn status(dir: &Path) -> Result<Status> {
    let settings = config::load(dir)?.alerts.monitor;
    Ok(classify(
        settings,
        alerts::last_check(dir),
        crate::timestamps::now_ns(),
    ))
}

/// Tocca fare il controllo? Sì se è acceso e l'ultimo è più vecchio dell'intervallo.
pub fn due(dir: &Path) -> bool {
    let Ok(settings) = config::load(dir).map(|c| c.alerts.monitor) else {
        return false;
    };
    if !settings.enabled {
        return false;
    }
    match status(dir) {
        Ok(s) => s
            .age_minutes
            .is_none_or(|age| age >= i64::from(settings.minutes)),
        Err(_) => true,
    }
}

/// Fa un controllo adesso: salva il risultato, avvisa chi di dovere e restituisce lo stato.
pub fn run(dir: &Path) -> Result<Status> {
    let _one_at_a_time = RUN.lock().unwrap_or_else(|p| p.into_inner());
    let problems = health::check(dir, health::DEFAULT_MAX_MINUTES, false)?;
    let change = alerts::after_cycle(dir, problems)?;
    crate::email::notify(dir, change);
    status(dir)
}

/// Una riga per il terminale e per `loglem status`.
pub fn line(dir: &Path) -> String {
    let Ok(s) = status(dir) else {
        return "Controllo automatico: stato non disponibile".into();
    };
    let at = s
        .checked_at
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    match s.state {
        "off" => "Controllo automatico: spento (si accende da Impostazioni)".into(),
        "never" => "Controllo automatico: non è ancora stato eseguito".into(),
        "stale" => format!(
            "Il controllo non gira da {} minuti (dovrebbe girare ogni {})",
            s.age_minutes.unwrap_or(0),
            s.minutes
        ),
        "problems" if s.open == 1 => format!("1 problema aperto, controllato alle {at}"),
        "problems" => format!("{} problemi aperti, controllato alle {at}", s.open),
        _ => format!("Tutto a posto, controllato alle {at}"),
    }
}

/// Cambia le impostazioni del controllo automatico.
pub fn configure(dir: &Path, enabled: Option<bool>, minutes: Option<u32>) -> Result<()> {
    config::edit(dir, |c| {
        let mut next = c.alerts.monitor;
        if let Some(enabled) = enabled {
            next.enabled = enabled;
        }
        if let Some(minutes) = minutes {
            next.minutes = minutes;
        }
        next.validate()?;
        c.alerts.monitor = next;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000_000_000;

    fn at(minutes_ago: i64) -> String {
        crate::timestamps::format_ns(NOW - minutes_ago * MINUTE_NS)
    }

    #[test]
    fn state_follows_age_and_open_problems() {
        let on = Settings::default();
        assert_eq!(classify(on, None, NOW).state, "never");
        assert_eq!(classify(on, Some((at(1), 0)), NOW).state, "ok");
        assert_eq!(classify(on, Some((at(2), 3)), NOW).state, "problems");
        // L'intervallo è 5 minuti: dopo più di 11 minuti il controllo è fermo.
        assert_eq!(classify(on, Some((at(11), 0)), NOW).state, "ok");
        assert_eq!(classify(on, Some((at(12), 0)), NOW).state, "stale");
        let off = Settings {
            enabled: false,
            minutes: 5,
        };
        assert_eq!(classify(off, Some((at(1), 0)), NOW).state, "off");
    }

    #[test]
    fn interval_is_checked() {
        assert!(Settings {
            enabled: true,
            minutes: 0
        }
        .validate()
        .is_err());
        assert!(Settings {
            enabled: true,
            minutes: 1441
        }
        .validate()
        .is_err());
        assert!(Settings {
            enabled: true,
            minutes: 1440
        }
        .validate()
        .is_ok());
        assert!(Settings::default().is_default());
    }

    #[test]
    fn due_only_when_the_last_check_is_old_enough() {
        let dir = tempfile::tempdir().unwrap();
        assert!(due(dir.path()), "never checked: due");
        let state = serde_json::json!({"checked_at": crate::timestamps::format_ns(crate::timestamps::now_ns()), "ok": true, "problems": []});
        std::fs::write(dir.path().join("health.json"), state.to_string()).unwrap();
        assert!(!due(dir.path()), "just checked: not due");
        let old = serde_json::json!({"checked_at": at_now(-6), "ok": true, "problems": []});
        std::fs::write(dir.path().join("health.json"), old.to_string()).unwrap();
        assert!(
            due(dir.path()),
            "six minutes ago with a five minute interval: due"
        );
        configure(dir.path(), Some(false), None).unwrap();
        assert!(!due(dir.path()), "switched off: never due");
    }

    fn at_now(offset_minutes: i64) -> String {
        crate::timestamps::format_ns(crate::timestamps::now_ns() + offset_minutes * MINUTE_NS)
    }
}
