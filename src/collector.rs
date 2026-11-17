use crate::{
    archive::Archive,
    config,
    model::{GapStatus, Input, Source, SourceStatus},
    query::Query,
    sources, timestamps, util,
};
use anyhow::{bail, Result};
use std::{collections::HashMap, fs, path::Path};

pub fn once(dir: &Path) -> Result<()> {
    util::private_dir(dir)?;
    let _lock = util::Lock::acquire(&dir.join("collector.lock"))?;
    cycle(dir)
}

fn cycle(dir: &Path) -> Result<()> {
    let _access = util::DataAccess::read(dir)?;
    let config = config::load(dir)?;
    let mut failures = Vec::new();
    for source in &config.sources {
        let path = config::archive_path(dir, &source.id);
        let result = Archive::open(&path)
            .and_then(|mut archive| collect_source(dir, &mut archive, source, config.poll_seconds));
        if let Err(error) = result {
            if let Ok(archive) = Archive::open(&path) {
                let _ = archive.set_state("phase", "error");
                let _ = archive.set_state("error", &format!("{error:#}"));
            }
            crate::diagnostics::error("collector", "collect_source", Some(&source.id), &error);
            failures.push(format!("{}: {error:#}", source.id));
        }
    }
    if let Err(e) = crate::rules::maintain(dir, &config) {
        crate::diagnostics::error("rules", "storage", None, &e);
        failures.push(e.to_string());
    }
    if !failures.is_empty() {
        bail!("{}", failures.join("\n"));
    }
    Ok(())
}

/// Senza letture riuscite da più di così, Loglem era fermo (o la sorgente in errore).
fn stop_threshold(poll_seconds: u64) -> i64 {
    (poll_seconds as i64 * 3).max(60) * 1_000_000_000
}

/// Un giro su una sorgente: lettura, scadenza delle copie, pulizia degli originali.
fn collect_source(
    dir: &Path,
    archive: &mut Archive,
    source: &Source,
    poll_seconds: u64,
) -> Result<()> {
    archive.set_state("source_definition", &serde_json::to_string(&source.input)?)?;
    let now = timestamps::now_ns();
    archive.set_state("phase", "collecting")?;
    // La scadenza vale anche per le sorgenti in pausa: la pausa non allunga la conservazione.
    let cutoff = source.period().cutoff(now);
    if !source.enabled || source.detached {
        expire(dir, archive, source, cutoff, now)?;
        archive.set_state("phase", "paused")?;
        archive.set_state("status", "Raccolta in pausa; conservazione ancora attiva")?;
        return Ok(());
    }
    let stopped_since = archive
        .get_state("last_success")?
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|last| now - last > stop_threshold(poll_seconds));

    let collected = read_source(archive, source, now, cutoff, stopped_since);
    // Prima la scadenza: un problema con la lettura o con gli originali non deve bloccarla.
    crate::privacy::apply(archive, &source.privacy, now)?;
    crate::proof::seal(dir, source, archive, now)?;
    expire(dir, archive, source, cutoff, now)?;
    let count = match collected {
        Ok(count) => count,
        Err(error) => {
            archive.set_state("phase", "error")?;
            archive.set_state("error", &format!("{error:#}"))?;
            archive.set_state(
                "status",
                "Lettura interrotta. I log già salvati restano disponibili.",
            )?;
            return Err(error);
        }
    };

    let mut status = match stopped_since {
        Some(since) => {
            let text = format!(
                "Ripresa dopo {} di fermo: recuperate {count} righe",
                util::human_duration(now - since)
            );
            archive.set_state("recovery", &format!("{now} {text}"))?;
            text
        }
        None => format!("Lettura completata; {count} nuove righe"),
    };
    let warning = match crate::management::cleanup_originals(source, archive) {
        Ok(cleanup) => {
            if cleanup.removed > 0 {
                status += &format!("; {} originali archiviati ed eliminati", cleanup.removed);
            }
            if cleanup.waiting > 0 {
                status += &format!(
                    "; {} originali verranno eliminati 24 ore dopo l'ultima modifica",
                    cleanup.waiting
                );
            }
            cleanup.warning()
        }
        Err(error) => Some(format!("Pulizia degli originali non riuscita: {error:#}")),
    };
    let warning = [warning, caddy_margin(source)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(". ");
    archive.set_state("warning", &warning)?;
    archive.set_state("last_success", &now.to_string())?;
    archive.set_state("phase", "ready")?;
    archive.set_state("error", "")?;
    archive.set_state("status", &status)?;
    Ok(())
}

/// Per i log di Caddy: avvisa se le regole di rotazione nel Caddyfile lasciano poco margine.
fn caddy_margin(source: &Source) -> Option<String> {
    let Input::File {
        path,
        rotated_pattern: Some(pattern),
    } = &source.input
    else {
        return None;
    };
    if !source.remove_rotated {
        return None;
    }
    let roll = crate::discover::caddy_roll_for(path)?;
    // I ruotati ancora presenti sono quelli non letti o in attesa delle 24 ore.
    let rotated = sources::file::candidates(path, Some(pattern))
        .ok()?
        .len()
        .saturating_sub(1);
    crate::discover::caddy_margin_warning(&roll, rotated)
}

/// Toglie le copie scadute. In modalità prova solo giorni interi già sigillati.
fn expire(dir: &Path, archive: &mut Archive, source: &Source, cutoff: i64, now: i64) -> Result<()> {
    if source.proof.is_some() || crate::proof::chain_path(dir, &source.id).exists() {
        crate::proof::expire(dir, source, archive, cutoff, now)
    } else {
        archive.retain_since(cutoff).map(|_| ())
    }
}

/// Legge le righe nuove della sorgente; restituisce quante sono state aggiunte.
fn read_source(
    archive: &mut Archive,
    source: &Source,
    now: i64,
    cutoff: i64,
    stopped_since: Option<i64>,
) -> Result<usize> {
    match &source.input {
        Input::Folder { path } => {
            let mut files = sources::file::folder_files(path)?;
            files.extend(sources::file::compressed_in_folder(path)?);
            note_vanished_files(archive, &files, stopped_since, now)?;
            let mut added = 0;
            let mut errors = Vec::new();
            for file in files {
                match sources::file::collect_any(archive, &file, now) {
                    Ok(count) => added += count,
                    Err(error) => errors.push(format!("{}: {error:#}", file.display())),
                }
            }
            if !errors.is_empty() {
                bail!("{}", errors.join("\n"));
            }
            Ok(added)
        }
        Input::File {
            path,
            rotated_pattern,
        } => {
            let files = sources::file::candidates(path, rotated_pattern.as_deref())?;
            if rotated_pattern.is_none() {
                note_vanished_files(archive, &files, stopped_since, now)?;
            }
            let mut added = 0;
            for file in files {
                added += sources::file::collect_any(archive, &file, now)?;
            }
            Ok(added)
        }
        Input::Journal(filter) => sources::journal::collect(archive, filter, now, cutoff),
        Input::Podman {
            container,
            connection,
        } => sources::podman::collect(
            archive,
            sources::podman::Runtime::Podman,
            container,
            connection.as_deref(),
            now,
            cutoff,
        ),
        Input::Docker { container, context } => sources::podman::collect(
            archive,
            sources::podman::Runtime::Docker,
            container,
            context.as_deref(),
            now,
            cutoff,
        ),
        Input::Kubernetes(target) => sources::kubernetes::collect(archive, target, now, cutoff),
    }
}

/// I file letti prima di un fermo e ora spariti (ruotati, rinominati o cancellati mentre
/// Loglem non girava) possono aver perso le ultime righe: lo si registra come lacuna.
fn note_vanished_files(
    archive: &Archive,
    files: &[std::path::PathBuf],
    stopped_since: Option<i64>,
    now: i64,
) -> Result<()> {
    let current: Vec<String> = files
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| sources::file::identity(&m))
        .collect();
    if let Some(since) = stopped_since {
        let previous: Vec<String> = archive
            .get_state("seen_files")?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let vanished = previous.iter().filter(|id| !current.contains(id)).count();
        if vanished > 0 {
            archive.record_gap(
                since,
                now,
                &format!(
                    "{vanished} file letti prima del fermo non ci sono più (ruotati o rinominati): \
                     le righe scritte dopo l'ultima lettura possono mancare. Con il modello dei \
                     file ruotati Loglem li legge anche dopo la rotazione"
                ),
            )?;
        }
    }
    archive.set_state("seen_files", &serde_json::to_string(&current)?)
}

/// Avvisi della pulizia degli originali e della sorgente (es. journal), uniti.
fn warnings(store: &Archive) -> Result<Option<String>> {
    let mut all = Vec::new();
    for key in ["warning", sources::journal::WARNING] {
        if let Some(text) = store.get_state(key)?.filter(|s| !s.is_empty()) {
            all.push(text);
        }
    }
    Ok((!all.is_empty()).then(|| all.join(". ")))
}

fn gap_statuses(store: &Archive) -> Result<Vec<GapStatus>> {
    Ok(store
        .gaps()?
        .into_iter()
        .rev()
        .map(|gap| GapStatus {
            from: timestamps::format_ns(gap.from_ns),
            to: timestamps::format_ns(gap.to_ns),
            reason: gap.reason,
        })
        .collect())
}

/// Il messaggio di ripresa dopo un fermo, mostrato per 24 ore.
fn recent_recovery(store: &Archive) -> Result<Option<String>> {
    let Some(saved) = store.get_state("recovery")? else {
        return Ok(None);
    };
    let Some((at, text)) = saved.split_once(' ') else {
        return Ok(None);
    };
    let age = timestamps::now_ns() - at.parse::<i64>().unwrap_or(0);
    Ok((age < 86_400 * 1_000_000_000).then(|| text.to_owned()))
}

pub fn statuses(dir: &Path) -> Result<Vec<SourceStatus>> {
    let _access = util::DataAccess::read(dir)?;
    config::load(dir)?
        .sources
        .into_iter()
        .map(|source| {
            let archive_path = config::archive_path(dir, &source.id);
            let (count, earliest, last, status) = if archive_path.exists() {
                let store = Archive::open(&archive_path)?;
                let (count, earliest) = store.stats()?;
                let last = store
                    .get_state("last_success")?
                    .and_then(|s| s.parse().ok())
                    .map(timestamps::format_ns);
                let status = store
                    .get_state("status")?
                    .unwrap_or_else(|| "Non ancora letta".into());
                (count, earliest.map(timestamps::format_ns), last, status)
            } else {
                (0, None, None, "Non ancora letta".into())
            };
            let (origin, bytes) = match &source.input {
                Input::Folder { path } => {
                    let bytes = sources::file::folder_files(path).ok().and_then(|files| {
                        files
                            .iter()
                            .map(|p| fs::metadata(p).ok().map(|m| m.len()))
                            .sum::<Option<u64>>()
                    });
                    (
                        format!("{} (file .log nella cartella)", path.display()),
                        bytes,
                    )
                }
                Input::File {
                    path,
                    rotated_pattern,
                } => {
                    let bytes = sources::file::candidates(path, rotated_pattern.as_deref())
                        .ok()
                        .and_then(|paths| {
                            paths
                                .iter()
                                .map(|p| fs::metadata(p).map(|m| m.len()).ok())
                                .collect::<Option<Vec<_>>>()
                        })
                        .map(|sizes| sizes.iter().sum());
                    (path.display().to_string(), bytes)
                }
                Input::Journal(filter) => {
                    (format!("Journal di systemd · {}", filter.describe()), None)
                }
                Input::Docker { container, context } => (
                    format!(
                        "Docker {} / {container}",
                        context.as_deref().unwrap_or("contesto predefinito")
                    ),
                    None,
                ),
                Input::Kubernetes(target) => (target.describe(), None),
                Input::Podman {
                    container,
                    connection,
                } => (
                    format!(
                        "Podman {} / {}",
                        connection.as_deref().unwrap_or("default connection"),
                        container
                    ),
                    None,
                ),
            };
            let (phase, error, warning, gaps, recovery) = if archive_path.exists() {
                let store = Archive::open(&archive_path)?;
                (
                    store
                        .get_state("phase")?
                        .unwrap_or_else(|| "waiting".into()),
                    store.get_state("error")?.filter(|s| !s.is_empty()),
                    warnings(&store)?,
                    gap_statuses(&store)?,
                    recent_recovery(&store)?,
                )
            } else {
                ("waiting".into(), None, None, Vec::new(), None)
            };
            Ok(SourceStatus {
                journal_runtime: source.journal_runtime,
                class: source.class.clone(),
                site: source.site.clone(),
                proof: source.proof.clone(),
                input: source.input.clone(),
                archive_dir: source.archive_dir.clone(),
                label: source.label.clone().unwrap_or_else(|| source.id.clone()),
                retention: source.period(),
                log_kind: source.log_kind,
                detached: source.detached,
                completed_dir: source.completed_dir.clone(),
                remove_rotated: source.remove_rotated,
                warning,
                gaps,
                recovery,
                phase,
                error,
                source: source.id,
                enabled: source.enabled,
                origin,
                archive: archive_path.display().to_string(),
                original_bytes: bytes,
                archive_bytes: util::bytes_on_disk(&archive_path),
                entries: count,
                earliest,
                last_collection: last,
                message: status,
                retention_days: source.retention_days,
            })
        })
        .collect()
}

pub fn search(
    dir: &Path,
    source: Option<&str>,
    query: &Query,
    from: i64,
    until: i64,
    limit: usize,
) -> Result<Vec<crate::model::SearchRow>> {
    let _access = util::DataAccess::read(dir)?;
    if from >= until {
        bail!("The start date must be before the end date");
    }
    let config = config::load(dir)?;
    if let Some(id) = source {
        if !config.sources.iter().any(|s| s.id == id) {
            bail!("Source not found");
        }
    }
    let mut rows = Vec::new();
    for s in config
        .sources
        .iter()
        .filter(|s| source.is_none_or(|id| s.id == id))
    {
        let path = config::archive_path(dir, &s.id);
        if path.exists() {
            rows.extend(Archive::open(&path)?.search(&s.id, query, from, until, limit)?);
            rows.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            rows.truncate(limit.min(1000));
        }
    }
    Ok(rows)
}

/// Righe arrivate dopo i punti indicati, per la diretta (pagina web e `loglem tail`).
///
/// Senza punti di partenza restituisce solo i punti attuali: la diretta mostra
/// le righe che arrivano da quel momento in poi.
pub fn tail(
    dir: &Path,
    source: Option<&str>,
    query: &Query,
    after: Option<&HashMap<String, i64>>,
) -> Result<(Vec<crate::model::SearchRow>, HashMap<String, i64>)> {
    const LIMIT: usize = 500;
    let _access = util::DataAccess::read(dir)?;
    let config = config::load(dir)?;
    let mut rows = Vec::new();
    let mut next = HashMap::new();
    for s in config
        .sources
        .iter()
        .filter(|s| source.is_none_or(|id| s.id == id))
    {
        let path = config::archive_path(dir, &s.id);
        if !path.exists() {
            continue;
        }
        let archive = Archive::open(&path)?;
        let (found, last) = match after {
            None => (Vec::new(), archive.rows_after(&s.id, i64::MAX, query, 0)?.1),
            Some(after) => {
                let start = after.get(&s.id).copied().unwrap_or(0);
                archive.rows_after(&s.id, start, query, LIMIT)?
            }
        };
        rows.extend(found);
        next.insert(s.id.clone(), last);
    }
    rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok((rows, next))
}
