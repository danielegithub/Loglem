//! Controllo di salute: Loglem è in funzione, le sorgenti vengono lette, nessuna lacuna recente.
//! Usato da `loglem check` (con codice di uscita), dalla pagina web e dal menu del terminale.
use crate::{collector, model::SourceStatus, util};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// Oltre questo tempo senza letture riuscite una sorgente è considerata ferma.
pub const DEFAULT_MAX_MINUTES: u64 = 15;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Problem {
    /// Nome stabile, per «segna come visto» e per capire se qualcosa è cambiato.
    pub id: String,
    pub source: Option<String>,
    pub text: String,
}

fn age_minutes(timestamp: &str, now: i64) -> Option<i64> {
    let then = chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()?
        .timestamp_nanos_opt()?;
    Some((now - then) / 60_000_000_000)
}

/// Elenca i problemi. `service_check` = controlla anche che Loglem sia in funzione
/// (inutile quando a chiedere è Loglem stesso).
pub fn check(dir: &Path, max_minutes: u64, service_check: bool) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    // Se il blocco del servizio è libero, nessun Loglem sta raccogliendo.
    if service_check && util::Lock::acquire(&dir.join("service.lock")).is_ok() {
        problems.push(Problem {
            id: "loglem-fermo".into(),
            source: None,
            text: "Loglem non è in esecuzione: nessuna sorgente viene letta".into(),
        });
    }
    problems.extend(self::problems(&collector::statuses(dir)?, max_minutes));
    problems.extend(crate::alerts::evaluate(dir)?);
    Ok(problems)
}

/// I problemi delle sorgenti, a partire dal loro stato già letto.
pub fn problems(statuses: &[SourceStatus], max_minutes: u64) -> Vec<Problem> {
    let mut problems = Vec::new();
    let now = crate::timestamps::now_ns();
    for status in statuses {
        if !status.enabled || status.detached {
            continue;
        }
        let name = status.label.clone();
        let mut add = |kind: &str, text: String| {
            problems.push(Problem {
                id: format!("{kind}:{}", status.source),
                source: Some(status.source.clone()),
                text: format!("{name}: {text}"),
            })
        };
        if let Some(error) = &status.error {
            add("errore", format!("lettura non riuscita: {error}"));
        }
        match status
            .last_collection
            .as_deref()
            .and_then(|t| age_minutes(t, now))
        {
            Some(minutes) if minutes as u64 > max_minutes => add(
                "ferma",
                format!("nessuna lettura riuscita da {minutes} minuti"),
            ),
            None if status.error.is_none() => add("mai-letta", "non è ancora stata letta".into()),
            _ => {}
        }
        for gap in &status.gaps {
            if age_minutes(&gap.to, now).is_some_and(|m| m < 24 * 60) {
                add(
                    &format!("lacuna:{}", gap.from),
                    format!("possibile lacuna fino a {}: {}", gap.to, gap.reason),
                );
            }
        }
    }
    problems
}
