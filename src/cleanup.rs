//! Chi ripulisce i file originali: Loglem, logrotate, il programma stesso o nessuno.
//! Dice se due sistemi cancellano gli stessi file, e se la rotazione del sistema non
//! coincide con quello che è impostato in Loglem. Solo lettura: non cambia niente.
use crate::{
    copies,
    logrotate::{Lookup, Rules},
    model::{Input, Retention, RetentionUnit, Source},
    sources::file,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct System {
    /// `logrotate`, `program`, `none` (nessuna regola trovata) o `unknown` (non si può sapere).
    pub kind: &'static str,
    pub text: String,
    /// Per quanti giorni circa restano gli originali, se il sistema lo dice.
    pub days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Note {
    /// `ok`, `info` o `warn`.
    pub level: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Solo le sorgenti che leggono file hanno originali da ripulire.
    pub applies: bool,
    pub matching_files: Option<Vec<String>>,
    pub loglem_on: bool,
    pub system: System,
    pub level: &'static str,
    pub headline: String,
    pub notes: Vec<Note>,
    pub pattern: Option<String>,
    pub suggested_pattern: Option<String>,
    /// Loglem può cancellare i vecchi file di questa sorgente (serve un modello dei nomi).
    pub can_enable_rotated: bool,
    pub keep_days: u32,
}

/// Tutto quello che serve per decidere, già raccolto: così la decisione si prova a tavolino.
pub struct Facts {
    pub loglem_rotated: bool,
    pub loglem_handoff: bool,
    pub is_folder: bool,
    pub pattern: Option<String>,
    pub system: System,
    pub keep_days: u32,
    pub rotated_present: bool,
    pub unread: Vec<String>,
    pub suggested: Option<String>,
}

fn days_of(retention: Retention) -> u32 {
    match retention.unit {
        RetentionUnit::Days => retention.amount,
        RetentionUnit::Months => retention.amount.saturating_mul(30),
        RetentionUnit::Years => retention.amount.saturating_mul(365),
    }
}

fn system_from_lookup(lookup: Lookup) -> System {
    match lookup {
        Lookup::Found(rule) => System {
            kind: "logrotate",
            text: format!("logrotate ({})", rule.describe()),
            days: rule.days(),
        },
        Lookup::NoRule => System {
            kind: "none",
            text: "nessuna regola logrotate trovata".into(),
            days: None,
        },
        Lookup::NotInstalled => System {
            kind: "unknown",
            text: "su questo computer non c'è logrotate".into(),
            days: None,
        },
    }
}

fn system_for(input: &Input, rules: Option<&Rules>) -> System {
    let lookup = |path: &Path| match rules {
        Some(rules) => rules.find(path),
        None => Lookup::NotInstalled,
    };
    match input {
        Input::File { path, .. } => {
            // Alcuni programmi ruotano da soli i propri file: è la stessa cosa.
            if let Some((_, roll)) = crate::recipes::rotation_for(path) {
                if let Some(days) = roll.keep_for_days {
                    return System {
                        kind: "program",
                        text: format!("il programma stesso (tiene i vecchi file {days:.0} giorni)"),
                        days: Some(days.ceil() as u32),
                    };
                }
                if let Some(count) = roll.keep {
                    return System {
                        kind: "program",
                        text: format!("il programma stesso (tiene gli ultimi {count} vecchi file)"),
                        days: None,
                    };
                }
            }
            system_from_lookup(lookup(path))
        }
        Input::Folder { path, .. } => {
            let files: Vec<PathBuf> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "log"))
                .take(50)
                .collect();
            if files.is_empty() || rules.is_none() {
                return system_from_lookup(if rules.is_none() {
                    Lookup::NotInstalled
                } else {
                    Lookup::NoRule
                });
            }
            let found: Vec<_> = files
                .iter()
                .filter_map(|p| match lookup(p) {
                    Lookup::Found(rule) => Some(rule),
                    _ => None,
                })
                .collect();
            if found.len() == files.len() {
                let days = found.iter().filter_map(|r| r.days()).max();
                System {
                    kind: "logrotate",
                    text: "logrotate, con una regola per ogni file della cartella".into(),
                    days,
                }
            } else {
                System {
                    kind: "none",
                    text: format!(
                        "logrotate ha una regola solo per {} file .log su {}",
                        found.len(),
                        files.len()
                    ),
                    days: None,
                }
            }
        }
        _ => System {
            kind: "none",
            text: String::new(),
            days: None,
        },
    }
}

/// Dal nome di un file ruotato accanto al log attivo ricava il modello dei nomi.
pub(crate) fn pattern_for(active: &str, rotated: &str) -> Option<String> {
    let plain = rotated.strip_suffix(".gz").unwrap_or(rotated);
    if plain
        .strip_prefix(active)
        .is_some_and(|s| s.starts_with('.'))
    {
        return Some(format!("{active}.*"));
    }
    if plain
        .strip_prefix(active)
        .is_some_and(|s| s.starts_with('-'))
    {
        return Some(format!("{active}-*"));
    }
    let stem = active.strip_suffix(".log")?;
    (plain.starts_with(&format!("{stem}-")) && plain.ends_with(".log"))
        .then(|| format!("{stem}-*.log"))
}

pub fn analyse(f: &Facts) -> (Report, Vec<Note>) {
    let loglem_on = f.loglem_rotated || f.loglem_handoff;
    let other = matches!(f.system.kind, "logrotate" | "program");
    let mut notes: Vec<Note> = Vec::new();
    let (level, headline) = match (loglem_on, f.system.kind) {
        (true, _) if other => {
            notes.push(Note {
                level: "warn",
                text: "Per evitare che due programmi cancellino gli stessi file, spegni la pulizia di Loglem scegliendo «Conserva i file originali».".into(),
            });
            (
                "warn",
                format!(
                    "Doppia pulizia: questi file li cancellano sia {} sia Loglem.",
                    f.system.text
                ),
            )
        }
        (true, kind) => {
            let what = if f.loglem_handoff && !f.loglem_rotated {
                "i log chiusi nella cartella scelta"
            } else {
                "i vecchi file accanto al log"
            };
            notes.push(Note {
                level: "info",
                text: "Loglem cancella solo file già archiviati e controllati riga per riga, fermi da 24 ore. Il file su cui il programma scrive non si tocca.".into(),
            });
            if kind == "none" && !f.rotated_present && f.loglem_rotated {
                notes.push(Note {
                    level: "warn",
                    text: "Non abbiamo trovato vecchi file o una regola che ne crei. Se il programma scrive sempre nello stesso file, quel file continuerà a crescere.".into(),
                });
            } else if kind == "unknown" {
                notes.push(Note {
                    level: "info",
                    text: "Loglem può pulire i vecchi file quando il programma chiude il log corrente e ne apre uno nuovo.".into(),
                });
            }
            ("ok", format!("Li ripulisce Loglem: cancella {what}, dopo averli archiviati e controllati."))
        }
        (false, _) if other => (
            "ok",
            format!(
                "Li ripulisce {}. Loglem non cancella niente: va bene così, qui non devi fare niente.",
                f.system.text
            ),
        ),
        (false, "none") => {
            notes.push(Note {
                level: "warn",
                text: if f.is_folder || f.pattern.is_some() {
                    "Controlla se il programma elimina già i vecchi file oppure accendi qui sotto la pulizia di Loglem.".into()
                } else {
                    "Per scegliere quali file leggere, apri Avanzate e controlla il modello dei nomi.".into()
                },
            });
            (
                "warn",
                "Non è stata trovata una regola di pulizia. Se i file si accumulano, continuano a crescere i consumi di spazio.".to_owned(),
            )
        }
        (false, _) => (
            "info",
            "Loglem conserva gli originali. Non è stato possibile stabilire se il programma li cancella.".to_owned(),
        ),
    };

    // La rotazione del sistema contro la durata scelta in Loglem.
    if let Some(days) = f.system.days {
        if days > f.keep_days {
            notes.push(Note {
                level: "warn",
                text: format!(
                    "I file originali restano circa {days} giorni, ma la copia di Loglem solo {}. Gli originali durano più della copia: se volevi tenere i dati {} giorni, accorcia la rotazione oppure allunga la durata in Loglem.",
                    f.keep_days, f.keep_days
                ),
            });
        } else {
            notes.push(Note {
                level: "ok",
                text: format!(
                    "Dopo circa {days} giorni restano solo le copie di Loglem, che durano {} giorni.",
                    f.keep_days
                ),
            });
        }
    }
    // File ruotati dal sistema con nomi che Loglem non legge.
    if !f.unread.is_empty() {
        let pattern = f
            .pattern
            .as_deref()
            .map(|p| format!(" Il modello attuale dei nomi è «{p}»."))
            .unwrap_or_else(|| " Per questa sorgente non c'è nessun modello dei nomi.".into());
        let advice = f
            .suggested
            .as_deref()
            .map(|s| format!(" Cambialo in «{s}»."))
            .unwrap_or_default();
        notes.push(Note {
            level: "warn",
            text: format!(
                "{} che Loglem non legge (per esempio «{}»).{pattern}{advice} Se il file ruota mentre Loglem è fermo, quelle righe si perdono.",
                if f.unread.len() == 1 {
                    "Accanto al log c'è un vecchio file".to_owned()
                } else {
                    format!("Accanto al log ci sono {} vecchi file", f.unread.len())
                },
                f.unread[0]
            ),
        });
    }

    let report = Report {
        applies: true,
        matching_files: None,
        loglem_on,
        system: f.system.clone(),
        level,
        headline,
        notes: notes.clone(),
        pattern: f.pattern.clone(),
        suggested_pattern: f.suggested.clone(),
        can_enable_rotated: !f.is_folder && f.pattern.is_some(),
        keep_days: f.keep_days,
    };
    (report, notes)
}

/// Il quadro per una sorgente. `rotated` e `handoff` provano uno stato non ancora salvato.
pub fn report(source: &Source, rotated: Option<bool>, handoff: Option<bool>) -> Report {
    let (path, pattern, is_folder) = match &source.input {
        Input::File {
            path,
            rotated_pattern,
        } => (Some(path.clone()), rotated_pattern.clone(), false),
        Input::Folder { path, .. } => (Some(path.clone()), None, true),
        _ => (None, None, false),
    };
    let Some(path) = path else {
        return Report {
            applies: false,
            matching_files: None,
            loglem_on: false,
            system: System {
                kind: "none",
                text: String::new(),
                days: None,
            },
            level: "info",
            headline: String::new(),
            notes: vec![],
            pattern: None,
            suggested_pattern: None,
            can_enable_rotated: false,
            keep_days: days_of(source.retention),
        };
    };
    let rules = Rules::load();
    let system = system_for(&source.input, rules.as_ref());

    let mut matching_files = None;
    let (mut rotated_present, mut unread, mut suggested) = (false, Vec::new(), None);
    if let (false, Some(name)) = (
        is_folder,
        path.file_name().and_then(|n| n.to_str()).map(str::to_owned),
    ) {
        let siblings = copies::visible_rotations(&path).unwrap_or_default();
        rotated_present = !siblings.is_empty();
        let candidates = file::candidates(&path, pattern.as_deref());
        matching_files = candidates.as_ref().ok().map(|paths| {
            paths
                .iter()
                .filter(|p| *p != &path)
                .filter_map(|p| p.file_name())
                .take(12)
                .map(|n| n.to_string_lossy().into_owned())
                .collect()
        });
        let covered = candidates.unwrap_or_default();
        let mut missing: Vec<String> = siblings
            .iter()
            .filter(|p| !covered.contains(p))
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            .collect();
        missing.sort();
        let guesses: Vec<_> = missing.iter().map(|m| pattern_for(&name, m)).collect();
        if let Some(Some(first)) = guesses.first() {
            if guesses.iter().all(|g| g.as_ref() == Some(first)) {
                suggested = Some(first.clone());
            }
        }
        unread = missing;
    }

    let facts = Facts {
        loglem_rotated: rotated.unwrap_or(source.remove_rotated),
        loglem_handoff: handoff.unwrap_or(source.completed_dir.is_some()),
        is_folder,
        pattern,
        system,
        keep_days: days_of(source.retention),
        rotated_present,
        unread,
        suggested,
    };
    let mut report = analyse(&facts).0;
    report.matching_files = matching_files;
    report
}

/// Una frase per `loglem status` e per le schede: il quadro e gli avvisi.
pub fn summary(source: &Source) -> String {
    let r = report(source, None, None);
    let mut parts = vec![r.headline];
    parts.extend(
        r.notes
            .into_iter()
            .filter(|n| n.level == "warn")
            .map(|n| n.text),
    );
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(kind: &'static str, days: Option<u32>) -> System {
        System {
            kind,
            text: match kind {
                "logrotate" => {
                    "logrotate (ruota ogni giorno, tiene 14 copie, circa 14 giorni)".into()
                }
                "none" => "nessuna regola logrotate trovata".into(),
                _ => "su questo computer non c'è logrotate".into(),
            },
            days,
        }
    }

    fn facts(rotated: bool, kind: &'static str, days: Option<u32>) -> Facts {
        Facts {
            loglem_rotated: rotated,
            loglem_handoff: false,
            is_folder: false,
            pattern: Some("access.log.*".into()),
            system: system(kind, days),
            keep_days: 28,
            rotated_present: true,
            unread: vec![],
            suggested: None,
        }
    }

    #[test]
    fn both_cleaning_is_a_warning_that_says_to_turn_loglem_off() {
        let (report, notes) = analyse(&facts(true, "logrotate", Some(14)));
        assert_eq!(report.level, "warn");
        assert!(report.headline.starts_with("Doppia pulizia"));
        assert!(notes
            .iter()
            .any(|n| n.text.contains("spegni la pulizia di Loglem")));
    }

    #[test]
    fn rotation_alone_needs_nothing_from_the_user() {
        let (report, _) = analyse(&facts(false, "logrotate", Some(14)));
        assert_eq!(report.level, "ok");
        assert!(report.headline.contains("qui non devi fare niente"));
    }

    #[test]
    fn nobody_cleaning_is_a_warning_with_two_ways_out() {
        let (report, notes) = analyse(&facts(false, "none", None));
        assert_eq!(report.level, "warn");
        assert!(report.headline.contains("continuano a crescere"));
        assert!(notes
            .iter()
            .any(|n| n.text.contains("accendi qui sotto la pulizia di Loglem")));
    }

    #[test]
    fn without_a_name_pattern_loglem_cleanup_asks_for_one_first() {
        let mut f = facts(false, "none", None);
        f.pattern = None;
        let (report, notes) = analyse(&f);
        assert!(!report.can_enable_rotated);
        assert!(notes.iter().any(|n| n.text.contains("modello dei nomi")));
    }

    #[test]
    fn loglem_alone_explains_what_it_deletes_and_that_nobody_rotates() {
        let mut f = facts(true, "none", None);
        f.rotated_present = false;
        let (report, notes) = analyse(&f);
        assert_eq!(report.level, "ok");
        assert!(report.headline.starts_with("Li ripulisce Loglem"));
        assert!(notes
            .iter()
            .any(|n| n.level == "warn" && n.text.contains("sempre nello stesso file")));
    }

    #[test]
    fn originals_outliving_the_loglem_copy_are_flagged() {
        let (_, notes) = analyse(&facts(false, "logrotate", Some(60)));
        assert!(notes
            .iter()
            .any(|n| n.level == "warn" && n.text.contains("restano circa 60 giorni")));
        let (_, notes) = analyse(&facts(false, "logrotate", Some(14)));
        assert!(notes.iter().all(|n| n.level != "warn"));
    }

    #[test]
    fn unread_rotated_files_get_a_suggested_pattern() {
        let mut f = facts(false, "logrotate", Some(14));
        f.unread = vec!["access.log-20261110.gz".into()];
        f.suggested = Some("access.log-*".into());
        let (report, notes) = analyse(&f);
        assert_eq!(report.suggested_pattern.as_deref(), Some("access.log-*"));
        assert!(notes.iter().any(|n| n.text.contains("«access.log-*»")));
    }

    #[test]
    fn patterns_follow_the_names_that_rotation_tools_produce() {
        assert_eq!(
            pattern_for("access.log", "access.log.1").as_deref(),
            Some("access.log.*")
        );
        assert_eq!(
            pattern_for("access.log", "access.log.2.gz").as_deref(),
            Some("access.log.*")
        );
        assert_eq!(
            pattern_for("access.log", "access.log-20261110").as_deref(),
            Some("access.log-*")
        );
        assert_eq!(
            pattern_for("access.log", "access-2026-11-10T00-00-00.000.log").as_deref(),
            Some("access-*.log")
        );
        assert_eq!(pattern_for("access.log", "other.log"), None);
    }
}
