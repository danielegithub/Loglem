use crate::{
    archive::Archive,
    config,
    model::{Input, SourceStatus},
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
    for source in config.sources {
        let path = config::archive_path(dir, &source.id);
        let result = (|| -> Result<()> {
            let mut archive = Archive::open(&path)?;
            let definition = serde_json::to_string(&source.input)?;
            match archive.get_state("source_definition")? {
                Some(previous) if previous != definition => {
                    archive.set_state("source_definition", &definition)?
                }
                None => archive.set_state("source_definition", &definition)?,
                _ => {}
            }
            let now = timestamps::now_ns();
            archive.set_state("phase", "collecting")?;
            // Retention continues for paused or missing sources; pausing does not extend it.
            let cutoff = source.period().cutoff(now);
            if !source.enabled || source.detached {
                archive.retain_since(cutoff)?;
                archive.set_state("phase", "paused")?;
                archive.set_state("status", "Raccolta in pausa; conservazione ancora attiva")?;
                return Ok(());
            }
            let collected = (|| -> Result<usize> {
                match &source.input {
                    Input::Folder { path } => {
                        let mut added = 0;
                        let mut errors = Vec::new();
                        let mut files = sources::file::folder_files(path)?;
                        files.extend(sources::file::compressed_in_folder(path)?);
                        for file in files {
                            match sources::file::collect_any(&mut archive, &file, now) {
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
                        let mut added = 0;
                        for file in sources::file::candidates(path, rotated_pattern.as_deref())? {
                            added += sources::file::collect_any(&mut archive, &file, now)?;
                        }
                        Ok(added)
                    }
                    Input::Journal(filter) => {
                        sources::journal::collect(&mut archive, filter, now, cutoff)
                    }
                    Input::Podman {
                        container,
                        connection,
                    } => sources::podman::collect(
                        &mut archive,
                        container,
                        connection.as_deref(),
                        now,
                        cutoff,
                    ),
                }
            })();
            match collected {
                Ok(count) => {
                    // Prima la scadenza: un problema con gli originali non deve bloccarla.
                    archive.retain_since(cutoff)?;
                    let mut status = format!("Lettura completata; {count} nuove righe");
                    let warning = match crate::management::cleanup_originals(&source, &mut archive)
                    {
                        Ok(cleanup) => {
                            if cleanup.removed > 0 {
                                status += &format!(
                                    "; {} originali archiviati ed eliminati",
                                    cleanup.removed
                                );
                            }
                            if cleanup.waiting > 0 {
                                status += &format!(
                                    "; {} originali verranno eliminati 24 ore dopo l'ultima modifica",
                                    cleanup.waiting
                                );
                            }
                            cleanup.warning()
                        }
                        Err(error) => {
                            Some(format!("Pulizia degli originali non riuscita: {error:#}"))
                        }
                    };
                    archive.set_state("warning", warning.as_deref().unwrap_or(""))?;
                    archive.set_state("last_success", &now.to_string())?;
                    archive.set_state("phase", "ready")?;
                    archive.set_state("error", "")?;
                    archive.set_state("status", &status)?;
                    Ok(())
                }
                Err(error) => {
                    archive.retain_since(cutoff)?;
                    archive.set_state("phase", "error")?;
                    archive.set_state("error", &format!("{error:#}"))?;
                    archive.set_state(
                        "status",
                        "Lettura interrotta. I log già salvati restano disponibili.",
                    )?;
                    Err(error)
                }
            }
        })();
        if let Err(error) = result {
            if let Ok(archive) = Archive::open(&path) {
                let _ = archive.set_state("phase", "error");
                let _ = archive.set_state("error", &format!("{error:#}"));
            }
            crate::diagnostics::error("collector", "collect_source", Some(&source.id), &error);
            failures.push(format!("{}: {error:#}", source.id));
        }
    }
    if !failures.is_empty() {
        bail!("{}", failures.join("\n"));
    }
    Ok(())
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
            let (phase, error, warning) = if archive_path.exists() {
                let store = Archive::open(&archive_path)?;
                (
                    store
                        .get_state("phase")?
                        .unwrap_or_else(|| "waiting".into()),
                    store.get_state("error")?.filter(|s| !s.is_empty()),
                    warnings(&store)?,
                )
            } else {
                ("waiting".into(), None, None)
            };
            Ok(SourceStatus {
                input: source.input.clone(),
                archive_dir: source.archive_dir.clone(),
                label: source.label.clone().unwrap_or_else(|| source.id.clone()),
                retention: source.period(),
                log_kind: source.log_kind,
                detached: source.detached,
                completed_dir: source.completed_dir.clone(),
                remove_rotated: source.remove_rotated,
                warning,
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
    text: &str,
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
            rows.extend(Archive::open(&path)?.search(&s.id, text, from, until, limit)?);
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
    text: &str,
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
            None => (Vec::new(), archive.rows_after(&s.id, i64::MAX, "", 0)?.1),
            Some(after) => {
                let start = after.get(&s.id).copied().unwrap_or(0);
                archive.rows_after(&s.id, start, text, LIMIT)?
            }
        };
        rows.extend(found);
        next.insert(s.id.clone(), last);
    }
    rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok((rows, next))
}
