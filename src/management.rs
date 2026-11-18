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
    pub pause_minutes: Option<u32>,
    pub detached: Option<bool>,
    pub remove_rotated: Option<bool>,
    /// Modello dei nomi dei file ruotati (es. `access.log.*`). Vuoto = nessuno.
    pub rotated_pattern: Option<String>,
    pub path: Option<PathBuf>,
    pub container: Option<String>,
    pub connection: Option<String>,
}

/// Un modello dei nomi dei file ruotati è sicuro solo se parte dal nome del log: così non
/// può includere altri file della stessa cartella, che Loglem poi cancellerebbe.
pub(crate) fn checked_pattern(active: &Path, pattern: &str) -> Result<Option<String>> {
    if pattern.is_empty() {
        return Ok(None);
    }
    let name = active
        .file_name()
        .and_then(|n| n.to_str())
        .context("Nome del file di log non valido")?;
    let stem = name.strip_suffix(".log").unwrap_or(name);
    if pattern.contains('/') || pattern.len() > 200 {
        bail!("Il modello dei nomi non può contenere «/»");
    }
    if !pattern.starts_with(stem) || stem.is_empty() {
        bail!("Il modello deve iniziare con «{stem}», il nome del file di log: così non include altri file della cartella");
    }
    glob::Pattern::new(pattern).context("Modello dei nomi non valido")?;
    Ok(Some(pattern.to_owned()))
}

/// Elimina una sorgente con il suo storico. Solo se è in pausa o scollegata: una sorgente
/// che sta ancora raccogliendo va prima fermata, altrimenti si perderebbero righe nuove.
pub fn remove(dir: &Path, name: &str) -> Result<()> {
    remove_with_backup(dir, name, false).map(|_| ())
}

pub fn remove_with_backup(dir: &Path, name: &str, backup: bool) -> Result<Option<PathBuf>> {
    let _access = util::DataAccess::write(dir)?;
    let _collector = util::Lock::acquire(&dir.join("collector.lock"))?;
    let source = config::source(dir, name)?;
    if source.enabled && !source.detached {
        bail!("Metti prima in pausa o scollega la sorgente, poi potrai eliminarla");
    }
    let target = if backup {
        let target = dir
            .join("backups")
            .join(format!("{name}-{}", crate::timestamps::now_ns()));
        crate::backup::create_locked(dir, &target, false)?;
        crate::backup::verify(&target)?;
        Some(target)
    } else {
        None
    };
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
        fs::remove_file(&file)
            .with_context(|| format!("Non riesco a eliminare {}", file.display()))?;
    }
    config::edit(dir, |config| {
        config.sources.retain(|s| s.id != name);
        Ok(target)
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
        source.retention = retention;
    }
    if let Some(kind) = change.log_kind {
        source.log_kind = kind;
    }
    if let Some(enabled) = change.enabled {
        source.enabled = enabled;
        source.resume_at = None;
        if enabled {
            source.detached = false;
        }
    }
    if let Some(detached) = change.detached {
        source.detached = detached;
        source.resume_at = None;
        source.enabled = !detached;
    }
    if let Some(minutes) = change.pause_minutes {
        anyhow::ensure!(
            change.enabled == Some(false) && !source.detached && (1..=43200).contains(&minutes),
            "Durata della pausa non valida"
        );
        source.resume_at =
            Some(crate::timestamps::now_ns() / 1_000_000_000 + i64::from(minutes) * 60);
    }
    if let Some(path) = change.path.filter(|p| !is_current_path(&source.input, p)) {
        let path = fs::canonicalize(path).context("La posizione della sorgente non esiste")?;
        match &mut source.input {
            Input::Folder { path: current, .. } => {
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
    if let Some(pattern) = change.rotated_pattern {
        let Input::File {
            path,
            rotated_pattern,
        } = &mut source.input
        else {
            bail!("Il modello dei file ruotati vale solo per un file singolo");
        };
        *rotated_pattern = checked_pattern(path, pattern.trim())?;
        if rotated_pattern.is_none() {
            source.remove_rotated = false;
        }
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
    if source.remove_rotated {
        if let Input::File {
            path,
            rotated_pattern: Some(pattern),
        } = &source.input
        {
            checked_pattern(path, pattern)?;
        }
    }
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
        let current = c
            .sources
            .iter_mut()
            .find(|s| s.id == source.id)
            .context("Sorgente non trovata")?;
        anyhow::ensure!(
            serde_json::to_value(&*current)? == serde_json::to_value(&old)?,
            "Le impostazioni sono cambiate durante il salvataggio. Riapri il log e riprova."
        );
        *current = source.clone();
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
        Input::Folder { path, .. } | Input::File { path, .. } => path == requested,
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
/// i file nella cartella dei conclusi e, se attivo, i file ruotati.
///
/// Ogni file è trattato da solo: un problema su uno non blocca gli altri.
pub fn cleanup_originals(source: &Source, store: &mut Archive) -> Result<Cleanup> {
    let mut report = Cleanup::default();
    let (active, active_files) = match &source.input {
        Input::File { path, .. } => (path.clone(), vec![path.clone()]),
        Input::Folder { path, pattern } => (
            path.clone(),
            sources::file::folder_matching(path, pattern.as_deref())?,
        ),
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
    store.set_state("deleted_originals", "1")?;
    util::sync_dir(file.parent().context("Cartella mancante")?)?;
    Ok(())
}

// ---------- nuova sorgente ----------

/// Una sorgente da aggiungere, come la descrivono pagina, terminale e procedura guidata.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct NewSource {
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub journal_runtime: Option<sources::podman::Runtime>,
    /// `file`, `folder`, `podman`, `docker`, `kubernetes` o `journal`.
    pub kind: String,
    /// L'identificatore: nella procedura guidata si sceglie al salvataggio.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Per le cartelle: i nomi dei file da leggere (vuoto = `*.log`).
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub days: Option<u32>,
    #[serde(default)]
    pub retention: Option<Retention>,
    #[serde(default)]
    pub log_kind: LogKind,
    #[serde(default)]
    pub archive_dir: Option<String>,
    #[serde(default)]
    pub journal: Option<sources::journal::Filter>,
    #[serde(default)]
    pub kubernetes: Option<sources::kubernetes::Target>,
    /// Il programma riconosciuto (id della ricetta), solo come etichetta.
    #[serde(default)]
    pub program: Option<String>,
    /// Il nome da mostrare (es. «example.com · Accessi»).
    #[serde(default)]
    pub label: Option<String>,
}

fn container_id(
    runtime: sources::podman::Runtime,
    wanted: &str,
    connection: Option<&str>,
) -> Result<String> {
    Ok(sources::podman::discover_with(runtime, connection)?
        .into_iter()
        .find(|c| c.id == wanted || c.name == wanted)
        .context("Container non trovato")?
        .id)
}

/// Che cosa leggere, dalla richiesta.
pub fn input_of(request: &NewSource) -> Result<Input> {
    Ok(match request.kind.as_str() {
        "folder" => {
            let path = fs::canonicalize(request.path.as_ref().context("Scegli una cartella")?)?;
            if !path.is_dir() {
                bail!("Scegli una cartella");
            }
            let pattern = match request.pattern.as_deref().map(str::trim) {
                None | Some("") => None,
                Some(p) => Some(sources::file::checked_pattern(p)?),
            };
            Input::Folder { path, pattern }
        }
        // «caddy» arriva da pagine e script precedenti: è un file come gli altri.
        "file" | "caddy" => {
            let path = fs::canonicalize(request.path.as_ref().context("Scegli un file")?)?;
            if !path.is_file() {
                bail!("Scegli un file");
            }
            Input::File {
                rotated_pattern: Some(rotation_pattern(&path)),
                path,
            }
        }
        "podman" => {
            let wanted = request
                .container
                .as_deref()
                .context("Scegli un container")?;
            Input::Podman {
                container: container_id(
                    sources::podman::Runtime::Podman,
                    wanted,
                    request.connection.as_deref(),
                )?,
                connection: request.connection.clone(),
            }
        }
        "docker" => {
            let wanted = request
                .container
                .as_deref()
                .context("Scegli un container")?;
            Input::Docker {
                container: container_id(
                    sources::podman::Runtime::Docker,
                    wanted,
                    request.connection.as_deref(),
                )?,
                context: request.connection.clone(),
            }
        }
        "kubernetes" => {
            let target = request.kubernetes.clone().context("Scegli un pod")?;
            target.validate()?;
            Input::Kubernetes(target)
        }
        "journal" => Input::Journal(request.journal.clone().unwrap_or_default()),
        other => bail!("Tipo di sorgente sconosciuto: {other}"),
    })
}

/// Aggiunge una sorgente e restituisce il sito a cui appartiene.
pub fn create(dir: &Path, request: NewSource) -> Result<String> {
    let input = input_of(&request)?;
    let archive_dir = match request
        .archive_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(path) => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                bail!("La cartella SQLite deve essere assoluta");
            }
            util::private_dir(&path)?;
            Some(fs::canonicalize(path)?)
        }
        None => None,
    };
    let retention = request
        .retention
        .unwrap_or_else(|| Retention::days(request.days.unwrap_or(28)));
    retention.validate()?;
    let source = Source {
        id: request.name.clone(),
        site: request.site.clone(),
        journal_runtime: request.journal_runtime,
        input,
        retention,
        log_kind: request.log_kind,
        archive_dir,
        program: request
            .program
            .clone()
            .filter(|p| crate::recipes::get(p).is_some()),
        label: request.label.clone().filter(|l| !l.trim().is_empty()),
        ..Default::default()
    };
    let mut candidate = config::load(dir)?;
    // Il sito può arrivare come nome: si controlla con il suo identificatore, come lo salverà `add`.
    let mut checked = source.clone();
    checked.site = config::assigned_site(&mut candidate, &source.site);
    if checked.site.is_empty() && matches!(checked.input, Input::Journal(_)) {
        checked.site = crate::model::SERVER_SITE.into();
    }
    candidate.sources.push(checked);
    config::validate(&candidate)?;
    if source.archive_path(dir).exists() {
        bail!("Esiste già un archivio con questo identificatore; scegli un nome diverso");
    }
    // Una sorgente nuova non ha un archivio da spostare: la raccolta delle altre può continuare.
    let _access = util::DataAccess::read(dir)?;
    let parent = source
        .archive_path(dir)
        .parent()
        .context("Cartella archivio mancante")?
        .to_owned();
    util::private_dir(&parent)?;
    let probe = parent.join(format!(".loglem-probe-{}", crate::timestamps::now_ns()));
    util::private_file(&probe)?.sync_all()?;
    fs::remove_file(probe)?;
    config::add(dir, source)
}

// ---------- nomi dei file ruotati ----------

/// Il modello dei nomi dei file ruotati di un log, ricavato dai file che ci sono già accanto
/// (`access.log.1`, `access.log-20261110.gz`, `access-2026-11-10T00-00-00.000.log`), per
/// qualsiasi programma o logrotate. Se non ce ne sono ancora: `access.log.*`.
pub fn rotation_pattern(log: &Path) -> String {
    let name = log
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for sibling in crate::copies::visible_rotations(log).unwrap_or_default() {
        let other = sibling
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if let Some(pattern) = crate::cleanup::pattern_for(&name, other) {
            *counts.entry(pattern).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(p, _)| p)
        .unwrap_or_else(|| format!("{}.*", glob::Pattern::escape(&name)))
}

/// Una sorgente per un singolo file, con i suoi file ruotati. La pulizia degli originali
/// resta spenta: si accende nelle impostazioni della sorgente.
pub fn file_source(id: String, log: &Path, retention: Retention) -> Result<Source> {
    let path = fs::canonicalize(log).context("Il file di log non esiste")?;
    if !path.is_file() {
        bail!("Scegli un file");
    }
    retention.validate()?;
    Ok(Source {
        id,
        retention,
        input: Input::File {
            rotated_pattern: Some(rotation_pattern(&path)),
            path,
        },
        ..Source::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_pause_survives_reopen_and_resumes_only_when_due() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let dir = temp.path().join("data");
        let file = temp.path().join("service.log");
        fs::write(&file, "one event\n")?;
        config::add(
            &dir,
            file_source("service".into(), &file, Retention::days(28))?,
        )?;
        let change = || Change {
            name: "service".into(),
            enabled: Some(false),
            pause_minutes: Some(60),
            ..Default::default()
        };
        apply(&dir, change())?;
        crate::collector::once(&dir)?;
        let paused = config::source(&dir, "service")?;
        assert!(!paused.enabled && paused.resume_at.is_some());
        config::edit(&dir, |cfg| {
            cfg.sources[0].resume_at = Some(1);
            Ok(())
        })?;
        crate::collector::once(&dir)?;
        let resumed = config::source(&dir, "service")?;
        assert!(resumed.enabled && resumed.resume_at.is_none());
        assert_eq!(Archive::open(&resumed.archive_path(&dir))?.stats()?.0, 1);
        apply(&dir, change())?;
        apply(
            &dir,
            Change {
                name: "service".into(),
                detached: Some(true),
                ..Default::default()
            },
        )?;
        assert!(config::source(&dir, "service")?.resume_at.is_none());
        Ok(())
    }

    #[test]
    fn failed_backup_preserves_source_and_successful_removal_keeps_verified_snapshot() -> Result<()>
    {
        let temp = tempfile::tempdir()?;
        let dir = temp.path().join("data");
        let file = temp.path().join("access.log");
        fs::write(&file, "event\n")?;
        let s = file_source("access".into(), &file, Retention::days(28))?;
        config::add(&dir, s.clone())?;
        crate::collector::once(&dir)?;
        apply(
            &dir,
            Change {
                name: s.id.clone(),
                enabled: Some(false),
                ..Default::default()
            },
        )?;
        fs::write(dir.join("backups"), "occupied")?;
        assert!(remove_with_backup(&dir, &s.id, true).is_err());
        assert!(config::source(&dir, &s.id)?.archive_path(&dir).is_file());
        fs::remove_file(dir.join("backups"))?;
        let snapshot = remove_with_backup(&dir, &s.id, true)?.unwrap();
        crate::backup::verify(&snapshot)?;
        assert!(!s.archive_path(&dir).exists() && config::load(&dir)?.sources.is_empty());
        assert_eq!(
            Archive::open_reader(&snapshot.join("archives/access.db"))?
                .stats()?
                .0,
            1
        );
        assert_eq!(fs::read_to_string(file)?, "event\n");
        Ok(())
    }

    #[test]
    fn rotated_logs_are_collected_without_enabling_cleanup() -> Result<()> {
        // Nomi con la data, come li crea la rotazione interna di alcuni programmi.
        let temp = tempfile::tempdir()?;
        let active = temp.path().join("access.log");
        let rotated = temp.path().join("access-2026-11-10.log");
        fs::write(&active, "active\n")?;
        fs::write(&rotated, "rotated\n")?;
        let source = file_source("access".into(), &active, Retention::days(28))?;
        assert!(!source.remove_rotated);
        assert!(closed_originals(&source)?.is_empty());
        let Input::File {
            path,
            rotated_pattern,
        } = &source.input
        else {
            panic!("Expected file source")
        };
        assert!(sources::file::candidates(path, rotated_pattern.as_deref())?
            .contains(&fs::canonicalize(rotated)?));
        Ok(())
    }
}
