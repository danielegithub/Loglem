//! Source lifecycle and verified archive relocation, shared by every interface.
use crate::{
    archive::Archive,
    config,
    model::{Input, LogKind, Retention, Source},
    sources, util,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// Un originale viene eliminato solo se nessuno lo modifica da almeno 24 ore.
pub const SETTLE_TIME: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub name: String,
    pub label: Option<String>,
    pub retention: Option<Retention>,
    pub log_kind: Option<LogKind>,
    /// Empty string selects the built-in archives directory.
    pub archive_dir: Option<String>,
    /// Empty string disables automatic completed-file cleanup.
    pub completed_dir: Option<String>,
    #[serde(default)]
    pub handoff_confirmed: bool,
    pub enabled: Option<bool>,
    pub detached: Option<bool>,
    pub remove_rotated: Option<bool>,
    pub path: Option<PathBuf>,
    pub container: Option<String>,
    pub connection: Option<String>,
}

/// Elimina una sorgente con il suo storico. Solo se è in pausa o scollegata: una sorgente
/// che sta ancora raccogliendo va prima fermata, altrimenti si perderebbero righe nuove.
pub fn remove(dir: &Path, name: &str) -> Result<()> {
    let _access = util::DataAccess::write(dir)?;
    let _collector = util::Lock::acquire(&dir.join("collector.lock"))?;
    let source = config::source(dir, name)?;
    if source.enabled && !source.detached {
        bail!("Metti prima in pausa o scollega la sorgente, poi potrai eliminarla");
    }
    let archive = source.archive_path(dir);
    let mut files = vec![
        archive.clone(),
        archive.with_extension("db-wal"),
        archive.with_extension("db-shm"),
        archive.with_extension("db-journal"),
        crate::proof::chain_path(dir, name),
        crate::proof::tsa_path(dir, name),
    ];
    files.retain(|f| f.exists());
    for file in files {
        fs::remove_file(&file).with_context(|| format!("Non riesco a eliminare {}", file.display()))?;
    }
    config::edit(dir, |config| {
        config.sources.retain(|s| s.id != name);
        Ok(())
    })
}

pub fn apply(dir: &Path, change: Change) -> Result<Source> {
    let old = config::source(dir, &change.name)?;
    let exclusive = change.container.is_some()
        || change.archive_dir.as_ref().is_some_and(|p| {
            let requested = if p.trim().is_empty() {
                dir.join("archives")
            } else {
                PathBuf::from(p.trim())
            };
            requested != old.archive_path(dir).parent().unwrap()
        });
    let _access = if exclusive {
        util::DataAccess::write(dir)?
    } else {
        util::DataAccess::read(dir)?
    };
    let _collector = if exclusive {
        Some(util::Lock::acquire(&dir.join("collector.lock"))?)
    } else {
        None
    };
    let mut source = old.clone();
    if let Some(label) = change.label {
        source.label = Some(label.trim().to_owned());
    }
    if let Some(retention) = change.retention {
        retention.validate()?;
        source.retention = Some(retention);
        if retention.unit == crate::model::RetentionUnit::Days {
            source.retention_days = retention.amount;
        }
    }
    if let Some(kind) = change.log_kind {
        source.log_kind = kind;
    }
    if let Some(enabled) = change.enabled {
        source.enabled = enabled;
        if enabled {
            source.detached = false;
        }
    }
    if let Some(detached) = change.detached {
        source.detached = detached;
        source.enabled = !detached;
    }
    if let Some(path) = change.path.filter(|p| !is_current_path(&source.input, p)) {
        let path = fs::canonicalize(path).context("La posizione della sorgente non esiste")?;
        match &mut source.input {
            Input::Folder { path: current } => {
                if !path.is_dir() {
                    bail!("Scegli una cartella");
                }
                *current = path;
            }
            Input::File { path: current, .. } => {
                if !path.is_file() {
                    bail!("Scegli un file .log");
                }
                *current = path;
            }
            Input::Podman { .. } => bail!("Per Podman scegli un container"),
            Input::Journal(_) | Input::Docker { .. } | Input::Kubernetes(_) => {
                bail!("Questa sorgente non ha un percorso da cambiare")
            }
        }
    }
    if let Some(container) = change.container {
        let Input::Podman {
            container: old_id,
            connection: old_connection,
        } = &source.input
        else {
            bail!("Questa sorgente non è Podman");
        };
        let connection = change
            .connection
            .filter(|s| !s.is_empty())
            .or_else(|| old_connection.clone());
        let found = sources::podman::discover(connection.as_deref())?
            .into_iter()
            .find(|c| c.id == container || c.name == container)
            .context("Container non trovato")?;
        if old.archive_path(dir).exists() {
            sources::podman::migrate_legacy(&Archive::open(&old.archive_path(dir))?, old_id)?;
        }
        source.input = Input::Podman {
            container: found.id,
            connection,
        };
    }
    if let Some(remove) = change.remove_rotated {
        if remove
            && !matches!(
                &source.input,
                Input::File {
                    rotated_pattern: Some(_),
                    ..
                }
            )
        {
            bail!("La pulizia dei file ruotati richiede un file con un modello di rotazione");
        }
        source.remove_rotated = remove;
    }
    if let Some(folder) = change.archive_dir {
        if folder.trim().is_empty() {
            source.archive_dir = None;
        } else {
            let path = PathBuf::from(folder.trim());
            if !path.is_absolute() {
                bail!("La cartella SQLite deve avere un percorso assoluto");
            }
            util::private_dir(&path)?;
            source.archive_dir = Some(fs::canonicalize(path)?);
        }
    }
    if let Some(folder) = change.completed_dir {
        if folder.trim().is_empty() {
            source.completed_dir = None;
        } else {
            let path = fs::canonicalize(folder.trim())
                .context("La cartella dei file conclusi non esiste")?;
            if !path.is_dir() {
                bail!("Scegli una cartella per i file conclusi");
            }
            if source.completed_dir.as_ref() != Some(&path) && !change.handoff_confirmed {
                bail!("Conferma che in questa cartella verranno consegnati soltanto file chiusi, sui quali il programma non scriverà più");
            }
            source.completed_dir = Some(path);
        }
    }
    let mut candidate = config::load(dir)?;
    *candidate
        .sources
        .iter_mut()
        .find(|s| s.id == source.id)
        .context("Sorgente non trovata")? = source.clone();
    config::validate(&candidate)?;
    let old_path = old.archive_path(dir);
    let new_path = source.archive_path(dir);
    if let Some(completed) = &source.completed_dir {
        if new_path.parent().is_some_and(|p| completed == p) {
            bail!("I file conclusi devono essere separati dall'archivio SQLite");
        }
    }
    let relocated = old_path != new_path && old_path.exists();
    if old_path != new_path {
        util::private_dir(new_path.parent().context("Cartella archivio mancante")?)?;
        if new_path.exists() {
            bail!("La destinazione contiene già un archivio con questo identificatore; nessun file è stato sovrascritto");
        }
        // Test write access even if the source has no archive yet.
        let probe = new_path.with_extension(format!("probe-{}", crate::timestamps::now_ns()));
        util::private_file(&probe)?.sync_all()?;
        fs::remove_file(probe)?;
    }
    if relocated {
        relocate(&old_path, &new_path)?;
    }
    if let Err(error) = config::edit(dir, |c| {
        *c.sources
            .iter_mut()
            .find(|s| s.id == source.id)
            .context("Sorgente non trovata")? = source.clone();
        Ok(())
    }) {
        if relocated {
            let _ = fs::remove_file(&new_path);
        }
        return Err(error);
    }
    if relocated {
        // All Loglem archive readers/writers are excluded by DataAccess above.
        for path in [
            &old_path,
            &PathBuf::from(format!("{}-wal", old_path.display())),
            &PathBuf::from(format!("{}-shm", old_path.display())),
        ] {
            if path.exists() {
                fs::remove_file(path).with_context(|| {
                    format!(
                        "Archivio spostato, ma non riesco a rimuovere {}",
                        path.display()
                    )
                })?;
            }
        }
        util::sync_dir(old_path.parent().unwrap())?;
    }
    Ok(source)
}

fn relocate(old: &Path, new: &Path) -> Result<()> {
    let stage = new
        .parent()
        .unwrap()
        .join(format!(".loglem-move-{}", crate::timestamps::now_ns()));
    util::private_dir(&stage)?;
    let staged = stage.join("archive.db");
    let result = (|| -> Result<()> {
        let old = Archive::open(old)?;
        old.db.execute(
            "VACUUM INTO ?1",
            [staged.to_str().context("Percorso SQLite non UTF-8")?],
        )?;
        let copied = Archive::open(&staged)?;
        let integrity: String = copied
            .db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" || copied.stats()? != old.stats()? {
            bail!("Verifica dell'archivio copiato fallita; originale conservato");
        }
        drop(copied);
        drop(old);
        fs::File::open(&staged)?.sync_all()?;
        fs::hard_link(&staged, new)
            .context("Non riesco a creare il nuovo archivio senza sovrascrivere file esistenti")?;
        util::sync_dir(new.parent().unwrap())?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(stage);
    result
}

fn is_current_path(input: &Input, requested: &Path) -> bool {
    match input {
        Input::Folder { path } | Input::File { path, .. } => path == requested,
        Input::Podman { .. } | Input::Journal(_) | Input::Docker { .. } | Input::Kubernetes(_) => {
            false
        }
    }
}

/// Risultato di un giro di pulizia degli originali.
#[derive(Debug, Default)]
pub struct Cleanup {
    pub removed: usize,
    /// File non ancora eliminabili perché modificati da meno di [`SETTLE_TIME`].
    pub waiting: usize,
    pub problems: Vec<String>,
}

impl Cleanup {
    /// Testo da mostrare nella scheda della sorgente, se c'è qualcosa da segnalare.
    pub fn warning(&self) -> Option<String> {
        (!self.problems.is_empty()).then(|| {
            format!(
                "Alcuni originali non sono stati eliminati: {}",
                self.problems.join("; ")
            )
        })
    }
}

/// Archivia, verifica ed elimina gli originali che la sorgente consegna come chiusi:
/// i file nella cartella dei conclusi e, se attivo, i file ruotati (es. Caddy).
///
/// Ogni file è trattato da solo: un problema su uno non blocca gli altri.
pub fn cleanup_originals(source: &Source, store: &mut Archive) -> Result<Cleanup> {
    let mut report = Cleanup::default();
    let (active, active_files) = match &source.input {
        Input::File { path, .. } => (path.clone(), vec![path.clone()]),
        Input::Folder { path } => (path.clone(), sources::file::folder_files(path)?),
        Input::Podman { .. } | Input::Journal(_) | Input::Docker { .. } | Input::Kubernetes(_) => {
            return Ok(report)
        }
    };
    let active_ids: Vec<String> = active_files
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| sources::file::identity(&m))
        .collect();

    for file in closed_originals(source)? {
        let result = (|| -> Result<bool> {
            let meta = fs::metadata(&file)?;
            if active_ids.contains(&sources::file::identity(&meta)) {
                bail!("è ancora il file su cui il programma scrive");
            }
            let age = SystemTime::now()
                .duration_since(meta.modified()?)
                .unwrap_or_default();
            if age < SETTLE_TIME {
                return Ok(false);
            }
            if sources::gzip::is_gzip(&file) {
                remove_compressed(store, &file)?;
            } else {
                sources::file::archive_completed(store, &active, &file, true)?;
            }
            Ok(true)
        })();
        match result {
            Ok(true) => report.removed += 1,
            Ok(false) => report.waiting += 1,
            Err(error) => report
                .problems
                .push(format!("{}: {error:#}", file.display())),
        }
    }
    Ok(report)
}

/// I file che la sorgente ha dichiarato chiusi per sempre.
fn closed_originals(source: &Source) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if let Some(completed) = &source.completed_dir {
        files.extend(sources::file::folder_files(completed)?);
    }
    if let Input::File {
        path,
        rotated_pattern: Some(pattern),
    } = &source.input
    {
        if source.remove_rotated {
            let rotated = sources::file::candidates(path, Some(pattern))?;
            files.extend(rotated.into_iter().filter(|p| p != path));
        }
    }
    Ok(files)
}

/// Un `.gz` si elimina solo se tutto il suo contenuto è già nell'archivio.
fn remove_compressed(store: &mut Archive, file: &Path) -> Result<()> {
    sources::gzip::collect(store, file, crate::timestamps::now_ns())?;
    if !sources::gzip::is_archived(store, file)? {
        bail!("il contenuto non risulta archiviato; originale conservato");
    }
    fs::remove_file(file)?;
    util::sync_dir(file.parent().context("Cartella mancante")?)?;
    Ok(())
}

// ---------- Caddy ----------

/// Modello dei file che Caddy crea ruotando `access.log`: `access-<data>.log`.
pub fn caddy_rotation_pattern(log: &Path) -> Result<String> {
    let stem = log
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Nome del file di log non valido")?;
    if log.extension().and_then(|s| s.to_str()) != Some("log") {
        bail!("Scegli il file .log su cui scrive Caddy (es. access.log)");
    }
    Ok(format!("{}-*.log", glob::Pattern::escape(stem)))
}

/// Sorgente per un log di Caddy: legge il file attivo e quelli ruotati,
/// poi elimina i ruotati dopo averli archiviati e verificati.
pub fn caddy_source(id: String, log: &Path, retention: Retention) -> Result<Source> {
    let path = fs::canonicalize(log).context("Il file di log di Caddy non esiste")?;
    if !path.is_file() {
        bail!("Scegli il file .log su cui scrive Caddy");
    }
    retention.validate()?;
    Ok(Source {
        id,
        log_kind: LogKind::Access,
        retention: Some(retention),
        remove_rotated: true,
        input: Input::File {
            rotated_pattern: Some(caddy_rotation_pattern(&path)?),
            path,
        },
        ..Source::default()
    })
}

/// Blocco da incollare nel Caddyfile perché la rotazione funzioni con Loglem.
pub fn caddy_snippet(log: &Path) -> String {
    format!(
        "log {{\n\toutput file {} {{\n\t\troll_size 50MiB\n\t\troll_keep 100\n\t\troll_keep_for 60d\n\t}}\n}}",
        log.display()
    )
}
