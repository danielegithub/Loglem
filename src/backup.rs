//! Backup e ripristino dei dati di Loglem: configurazione e archivi.
//!
//! Il backup usa `VACUUM INTO` di SQLite: una copia coerente anche mentre Loglem
//! raccoglie. Un manifesto registra righe e impronta di ogni file, così il backup si
//! può verificare prima di fidarsene.
//!
//! Attenzione alla conservazione: una riga scaduta sparisce dall'archivio, ma resta
//! in ogni backup fatto prima della scadenza, finché quel backup esiste.
use crate::{archive::Archive, config, util};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const MANIFEST: &str = "loglem-backup.json";

#[derive(Serialize, Deserialize)]
struct Manifest {
    created: String,
    version: String,
    config_sha256: String,
    archives: Vec<ArchiveCopy>,
    note: String,
}

#[derive(Serialize, Deserialize)]
struct ArchiveCopy {
    source: String,
    file: String,
    entries: i64,
    sha256: String,
}

/// Copia configurazione e archivi in `target`, che deve essere vuota o non esistere.
pub fn create(dir: &Path, target: &Path) -> Result<String> {
    if target.exists() && fs::read_dir(target)?.next().is_some() {
        bail!(
            "{} non è vuota: scegli una cartella nuova",
            target.display()
        );
    }
    util::private_dir(&target.join("archives"))?;
    let _access = util::DataAccess::read(dir)?;
    let config_bytes =
        fs::read(dir.join("config.json")).context("Nessuna configurazione da copiare")?;
    util::atomic_write(&target.join("config.json"), &config_bytes)?;

    let mut archives = Vec::new();
    for source in config::load(dir)?.sources {
        let path = config::archive_path(dir, &source.id);
        if !path.exists() {
            continue;
        }
        let file = format!("archives/{}.db", source.id);
        let copy = target.join(&file);
        Archive::open(&path)?.db.execute(
            "VACUUM INTO ?1",
            [copy.to_str().context("Percorso non UTF-8")?],
        )?;
        archives.push(ArchiveCopy {
            entries: Archive::open(&copy)?.stats()?.0,
            sha256: util::file_digest(&copy)?,
            source: source.id,
            file,
        });
    }
    let manifest = Manifest {
        created: crate::timestamps::format_ns(crate::timestamps::now_ns()),
        version: crate::VERSION.into(),
        config_sha256: util::digest(&config_bytes),
        note: "Le righe scadute nell'archivio restano in questo backup finché esiste.".into(),
        archives,
    };
    util::atomic_write(
        &target.join(MANIFEST),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(format!(
        "Backup creato in {}: configurazione e {} archivi.",
        target.display(),
        manifest.archives.len()
    ))
}

/// Controlla impronte, integrità SQLite e numero di righe di ogni archivio.
pub fn verify(target: &Path) -> Result<String> {
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(target.join(MANIFEST))
            .context("Questa cartella non contiene un backup di Loglem")?,
    )?;
    if util::digest(&fs::read(target.join("config.json"))?) != manifest.config_sha256 {
        bail!("config.json è stato modificato o è danneggiato");
    }
    let mut total = 0;
    for copy in &manifest.archives {
        let path = target.join(&copy.file);
        if util::file_digest(&path)? != copy.sha256 {
            bail!("{} è stato modificato o è danneggiato", copy.file);
        }
        // Sola lettura: controllare un backup non deve mai modificarlo.
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let entries: i64 = db.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
        if integrity != "ok" || entries != copy.entries {
            bail!("{} non supera il controllo di integrità", copy.file);
        }
        total += copy.entries;
    }
    Ok(format!(
        "Backup del {} valido: {} archivi, {total} righe.",
        manifest.created,
        manifest.archives.len()
    ))
}

/// Ripristina un backup verificato. Loglem deve essere fermo. Gli archivi tornano nella
/// cartella predefinita; con `replace` i dati attuali vengono sostituiti.
pub fn restore(dir: &Path, target: &Path, replace: bool) -> Result<String> {
    let checked = verify(target)?;
    let Ok(_service) = util::Lock::acquire(&dir.join("service.lock")) else {
        bail!("Ferma Loglem prima del ripristino (loglem service stop, o chiudi «loglem web»)");
    };
    let _access = util::DataAccess::write(dir)?;
    if !replace && config::load(dir).is_ok_and(|c| !c.sources.is_empty()) {
        bail!(
            "Ci sono già dei dati in {}: usa --replace per sostituirli",
            dir.display()
        );
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(target.join(MANIFEST))?)?;
    util::private_dir(&dir.join("archives"))?;
    for copy in &manifest.archives {
        let destination = dir.join("archives").join(format!("{}.db", copy.source));
        for stale in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{stale}", destination.display()));
        }
        fs::copy(target.join(&copy.file), &destination)?;
    }
    let mut restored: crate::model::Config =
        serde_json::from_slice(&fs::read(target.join("config.json"))?)?;
    for source in &mut restored.sources {
        source.archive_dir = None; // tutti nella cartella predefinita
    }
    config::validate(&restored)?;
    util::atomic_write(
        &dir.join("config.json"),
        &serde_json::to_vec_pretty(&restored)?,
    )?;
    Ok(format!("{checked}\nRipristinato in {}.", dir.display()))
}
