//! Durata configurata alla sorgente. Lo storico già raccolto ha una scadenza indipendente.
use super::podman::{self, Runtime};
use crate::{model::Input, process};
use std::{process::Command, time::Duration};

pub fn describe(input: &Input) -> String {
    match input {
        Input::Journal(f) => format!(
            "{} {}",
            journal(),
            super::journal::history(f)
                .unwrap_or_else(|_| "Storico leggibile non determinato.".into())
        ),
        Input::Podman {
            container,
            connection,
        } => container_policy(Runtime::Podman, container, connection.as_deref()),
        Input::Docker { container, context } => {
            container_policy(Runtime::Docker, container, context.as_deref())
        }
        Input::Kubernetes(_) => {
            "La rotazione dipende dal nodo e dal runtime. Durata in giorni non disponibile.".into()
        }
        Input::File { path, .. } => {
            if let Some((recipe, roll)) = crate::recipes::rotation_for(path) {
                if let Some(days) = roll.keep_for_days {
                    return format!(
                        "{}: limite configurato di {days} giorni per i file ruotati.",
                        recipe.name
                    );
                }
                if let Some(count) = roll.keep {
                    return format!(
                        "{}: limite di {count} file ruotati. Durata in giorni non rilevata.",
                        recipe.name
                    );
                }
            }
            rotation_text(path)
        }
        Input::Folder { path, .. } => folder_rotation_text(path),
    }
}

/// Chi ripulisce un file: una riga corta con un livello (`ok`, `info`, `warn`) e, a parte,
/// la spiegazione lunga con cosa fare. La riga corta si legge a colpo d'occhio.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OwnerNote {
    pub text: String,
    pub help: String,
    pub level: &'static str,
}

impl OwnerNote {
    pub fn new(text: impl Into<String>, level: &'static str, help: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            help: help.into(),
            level,
        }
    }
}

/// `rotated` = copia già ruotata, non il file su cui il programma scrive.
pub fn cleanup_owner(
    rules: Option<&crate::logrotate::Rules>,
    path: &std::path::Path,
    rotated: bool,
) -> OwnerNote {
    use crate::logrotate::Lookup;
    let lookup = match rules {
        Some(rules) => rules.find(path),
        None => Lookup::NotInstalled,
    };
    match (lookup, rotated) {
        (Lookup::Found(rule), _) => OwnerNote::new(
            match rule.days() {
                Some(days) => format!("logrotate · circa {days} giorni"),
                None => "logrotate".to_owned(),
            },
            "ok",
            format!("logrotate {}. Qui non devi fare niente.", rule.describe()),
        ),
        (Lookup::NoRule, false) => OwnerNote::new(
            "Nessuno · il file cresce",
            "warn",
            "Nessuna regola logrotate per questo file e Loglem non lo cancella. Cosa fare: configura la rotazione nel programma che lo scrive o in logrotate. Se il programma ruota già da sé, non serve altro.",
        ),
        (Lookup::NoRule, true) => OwnerNote::new(
            "Nessuno · restano sul disco",
            "warn",
            "Nessuna regola logrotate: queste copie restano finché non le togli tu, perché Loglem non le cancella. Cosa fare: aggiungi una regola logrotate, oppure accendi la pulizia di Loglem nelle impostazioni della sorgente.",
        ),
        (Lookup::NotInstalled, false) => OwnerNote::new(
            "Non so chi lo ripulisce",
            "info",
            "Su questo computer non c'è logrotate, quindi non so chi ruota il file. Se il programma che lo scrive ruota i file da sé, non serve altro; altrimenti il file continua a crescere.",
        ),
        (Lookup::NotInstalled, true) => OwnerNote::new(
            "Non so chi lo ripulisce",
            "info",
            "Su questo computer non c'è logrotate: controlla nel programma che ha creato queste copie chi le cancella.",
        ),
    }
}

fn rotation_text(path: &std::path::Path) -> String {
    use crate::logrotate::{lookup, Lookup};
    match lookup(path) {
        Lookup::Found(rule) => format!(
            "Chi ripulisce i file ruotati: logrotate ({}). Loglem non li cancella. Qui non devi fare niente.",
            rule.describe()
        ),
        Lookup::NoRule => "Nessuna rotazione trovata in logrotate: il file continua a crescere e Loglem non lo cancella. Cosa fare: aggiungi una regola logrotate per questo file, oppure imposta la rotazione nel programma che lo scrive. Se lo fa già da sé, non serve altro.".into(),
        Lookup::NotInstalled => "Su questo computer non c'è logrotate, quindi non so chi ruota il file. Se il programma che lo scrive ruota i file da sé, non serve altro. Altrimenti il file continua a crescere: imposta la rotazione nel programma.".into(),
    }
}

fn folder_rotation_text(path: &std::path::Path) -> String {
    use crate::logrotate::{Lookup, Rules};
    let Some(rules) = Rules::load() else {
        return "Su questo computer non c'è logrotate, quindi non so chi ruota i file della cartella. Se i programmi che li scrivono li ruotano da sé, non serve altro. Altrimenti i file crescono per sempre.".into();
    };
    let files: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .take(50)
        .collect();
    let without = files
        .iter()
        .filter(|p| matches!(rules.find(p), Lookup::NoRule))
        .count();
    if files.is_empty() {
        "Nella cartella non ci sono ancora file .log: non posso dire chi li ruota.".into()
    } else if without == 0 {
        "Chi ripulisce i file ruotati: logrotate, che ha una regola per ogni file .log della cartella. Loglem non li cancella. Qui non devi fare niente.".into()
    } else {
        format!(
            "Alcuni file .log della cartella ({without} su {}) non hanno una regola logrotate: continuano a crescere e Loglem non li cancella. Cosa fare: aggiungi la rotazione in logrotate o nel programma che li scrive. Se lo fa già da sé, non serve altro.",
            files.len()
        )
    }
}

fn container_policy(runtime: Runtime, container: &str, connection: Option<&str>) -> String {
    let Ok(logs) = podman::log_config(runtime, connection, container) else {
        return format!(
            "Configurazione dei log di {} non disponibile.",
            runtime.name()
        );
    };
    if logs.driver == "journald" {
        if logs.journald {
            return journal();
        }
        return "journald sul server del container. Durata non verificata su quel server.".into();
    }
    if logs.driver == "none" {
        return "Log del container disattivati.".into();
    }
    if let Some(size) = logs.max_size {
        format!(
            "Driver {}: limite di dimensione {size}. Durata in giorni non indicata.",
            logs.driver
        )
    } else {
        format!(
            "Driver {}: durata in giorni non indicata dal runtime.",
            logs.driver
        )
    }
}

fn journal() -> String {
    let Ok(config) = process::output(
        Command::new("systemd-analyze").args(["cat-config", "systemd/journald.conf"]),
        Duration::from_secs(3),
    ) else {
        return "Configurazione di journald non disponibile.".into();
    };
    journal_policy(&String::from_utf8_lossy(&config))
}

fn journal_policy(config: &str) -> String {
    let mut journal_section = false;
    let mut duration = "";
    let mut storage = "";
    let mut disk_limit = "";
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            journal_section = line == "[Journal]";
        } else if journal_section {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "MaxRetentionSec" => duration = value.trim(),
                    "Storage" => storage = value.trim(),
                    "SystemMaxUse" => disk_limit = value.trim(),
                    _ => {}
                }
            }
        }
    }
    if storage == "none" {
        return "journald: salvataggio disattivato.".into();
    }
    let mut policy = match duration {
        "" | "0" | "infinity" => {
            "journald: nessuna scadenza fissa in giorni. Lo storico dipende anche dallo spazio."
                .into()
        }
        value => {
            // Il parser di systemd riconosce durate composte e valori senza unità (secondi).
            let output = process::output(
                Command::new("systemd-analyze")
                    .env("LC_ALL", "C")
                    .args(["timespan", "--", value]),
                Duration::from_secs(3),
            );
            let micros = output.ok().and_then(|data| {
                String::from_utf8_lossy(&data).lines().find_map(|line| {
                    line.trim()
                        .strip_prefix("us:")
                        .or_else(|| line.trim().strip_prefix("μs:"))
                        .and_then(|v| v.trim().parse::<u64>().ok())
                })
            });
            match micros {
                Some(0) => "journald: nessuna scadenza fissa in giorni.".into(),
                Some(86_400_000_000) => "journald: massimo 1 giorno, spazio permettendo.".into(),
                Some(us) if us % 86_400_000_000 == 0 => format!(
                    "journald: massimo {} giorni, spazio permettendo.",
                    us / 86_400_000_000
                ),
                _ => format!(
                    "journald: MaxRetentionSec = {value}{} (limite configurato).",
                    if value.parse::<f64>().is_ok() {
                        " secondi"
                    } else {
                        ""
                    }
                ),
            }
        }
    };
    if !disk_limit.is_empty() && storage != "volatile" {
        policy.push_str(&format!(" Limite su disco (SystemMaxUse): {disk_limit}."));
    }
    if storage == "volatile" {
        policy.push_str(" I log in memoria scadono anche al riavvio.");
    }
    policy
}

fn cache_key(s: &crate::model::Source) -> anyhow::Result<String> {
    Ok(serde_json::to_string(&(
        &s.input,
        s.remove_rotated,
        &s.completed_dir,
        s.retention,
    ))?)
}

pub(crate) fn refresh(
    db: &crate::archive::Archive,
    s: &crate::model::Source,
    now: i64,
) -> anyhow::Result<()> {
    let key = cache_key(s)?;
    if let Some(raw) = db.get_state("original_retention")? {
        if let Ok((old, at, _)) = serde_json::from_str::<(String, i64, String)>(&raw) {
            if old == key && (0..600_000_000_000).contains(&now.saturating_sub(at)) {
                return Ok(());
            }
        }
    }
    // Per i file il quadro dice anche chi ripulisce e se due sistemi si sovrappongono.
    let value = if matches!(s.input, Input::File { .. } | Input::Folder { .. }) {
        crate::cleanup::summary(s)
    } else {
        originals(&s.input, s.remove_rotated, s.completed_dir.is_some())
    };
    db.set_state(
        "original_retention",
        &serde_json::to_string(&(key, now, value))?,
    )
}

pub(crate) fn cached(dir: &std::path::Path, s: &crate::model::Source) -> anyhow::Result<String> {
    let path = s.archive_path(dir);
    if path.exists() {
        if let Some(raw) =
            crate::archive::Archive::open_reader(&path)?.get_state("original_retention")?
        {
            if let Ok((key, _, value)) = serde_json::from_str::<(String, i64, String)>(&raw) {
                if key == cache_key(s)? {
                    return Ok(value);
                }
            }
        }
    }
    Ok("Verifica prevista al prossimo ciclo di raccolta.".into())
}

pub fn originals(input: &Input, rotated: bool, handoff: bool) -> String {
    let mut parts = Vec::new();
    if rotated {
        parts.push("Pulizia attiva dei file ruotati. Il file attivo resta.".to_owned());
    }
    if handoff {
        parts.push("Pulizia attiva nella cartella dei file conclusi.".to_owned());
    }
    if rotated || handoff {
        parts.push("Loglem elimina questi file dopo la verifica e almeno 24 ore dall’ultima modifica. Dopo la rimozione resta la copia in Loglem fino alla sua scadenza, oltre a eventuali backup.".into());
    } else {
        parts.push("Loglem non cancella gli originali: la pulizia è spenta.".into());
    }
    parts.push(describe(input));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_uses_last_active_assignment_in_journal_section() {
        let config = "[Journal]\nMaxRetentionSec=30day\n#MaxRetentionSec=15day\n[Other]\nStorage=none\n[Journal]\nMaxRetentionSec=0\nStorage=volatile";
        let policy = journal_policy(config);
        assert!(policy.contains("nessuna scadenza fissa"));
        assert!(policy.contains("riavvio"));
    }
}
