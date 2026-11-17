//! Durata configurata alla sorgente. Lo storico già raccolto ha una scadenza indipendente.
use super::podman::{self, Runtime};
use crate::{model::Input, process};
use std::{process::Command, time::Duration};

pub fn describe(input: &Input) -> String {
    match input {
        Input::Journal(f) => format!("{} {}", journal(), super::journal::history(f).unwrap_or_else(|_|"Storico leggibile non determinato.".into())),
        Input::Podman { container, connection } => container_policy(Runtime::Podman, container, connection.as_deref()),
        Input::Docker { container, context } => container_policy(Runtime::Docker, container, context.as_deref()),
        Input::Kubernetes(_) => "La rotazione dipende dal nodo e dal runtime. Durata in giorni non disponibile.".into(),
        Input::File { path, .. } => {
            if let Some(roll) = crate::discover::caddy_roll_for(path) {
                if let Some(days) = roll.keep_for_days {
                    return format!("Caddy: limite configurato di {days} giorni per i file ruotati.");
                }
                if let Some(count) = roll.keep {
                    return format!("Caddy: limite di {count} file ruotati. Durata in giorni non rilevata.");
                }
            }
            "Dipende dalla rotazione impostata dal programma che scrive il file. Durata in giorni non rilevata.".into()
        }
        Input::Folder { .. } => "Ogni file segue la rotazione del programma che lo scrive. Durata della cartella non rilevabile in giorni.".into(),
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
        parts.push("Loglem lascia gli originali alla sorgente.".into());
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
