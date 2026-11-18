use crate::{
    model::{Config, Input, Retention, Site, Source, SERVER_SITE},
    util::{atomic_write, private_dir, Lock},
};
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn default_directory() -> Result<PathBuf> {
    directory_for_executable(&env::current_exe()?)
}

pub fn directory_for_executable(executable: &Path) -> Result<PathBuf> {
    let parent = executable
        .parent()
        .context("Percorso dell'eseguibile non disponibile")?;
    // Development builds keep data in the project, outside disposable target/.
    for candidate in parent.ancestors().take(4) {
        if candidate.file_name().and_then(|s| s.to_str()) == Some("target") {
            if let Some(project) = candidate.parent() {
                if project.join("Cargo.toml").is_file() {
                    return Ok(project.join("data"));
                }
            }
        }
    }
    Ok(parent.join("data"))
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        bail!("Source names must contain 1–64 letters, numbers, '-' or '_'");
    }
    Ok(())
}

pub fn validate(config: &Config) -> Result<()> {
    if config.version != 2 {
        bail!("Unsupported configuration version {}", config.version);
    }
    ensure!(
        config.sites.contains_key(SERVER_SITE),
        "Manca il sito di sistema"
    );
    for (id, site) in &config.sites {
        ensure!(
            valid_site_id(id) && site.id == *id,
            "Identificatore del sito non valido"
        );
        ensure!(
            !site.name.is_empty()
                && site.name.len() <= 200
                && !site.name.chars().any(char::is_control),
            "Nome del sito non valido"
        );
    }
    config.rules.validate(&config.sources)?;
    config.access.validate()?;
    config.alerts.validate()?;
    if !(1..=3600).contains(&config.poll_seconds) {
        bail!("Polling must be between 1 and 3600 seconds");
    }
    let mut ids = std::collections::HashSet::new();
    for source in &config.sources {
        validate_id(&source.id)?;
        if !ids.insert(&source.id) {
            bail!("Duplicate source {}", source.id);
        }
        ensure!(
            source.site.is_empty() || config.sites.contains_key(&source.site),
            "Sito sconosciuto per {}",
            source.id
        );
        source.retention.validate()?;
        source.privacy.validate(source)?;
        if source.label.as_ref().is_some_and(|s| {
            s.trim().is_empty() || s.len() > 120 || s.chars().any(char::is_control)
        }) {
            bail!("Nome visualizzato non valido");
        }
        if source
            .archive_dir
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            bail!("La cartella SQLite deve avere un percorso assoluto");
        }
        if let Some(completed) = &source.completed_dir {
            if !completed.is_absolute() {
                bail!("La cartella dei file conclusi deve essere assoluta");
            }
            match &source.input {
                Input::Folder { path, .. } if path == completed => {
                    bail!("La cartella dei file conclusi deve essere diversa dalla cartella attiva")
                }
                Input::File { path, .. } if path.parent() == Some(completed.as_path()) => {
                    bail!("La cartella dei file conclusi deve essere separata dal file attivo")
                }
                Input::Podman { .. }
                | Input::Journal(_)
                | Input::Docker { .. }
                | Input::Kubernetes(_) => {
                    bail!("La pulizia dei file conclusi vale solo per sorgenti file")
                }
                _ => {}
            }
        }
        match &source.input {
            Input::Journal(filter) => filter.validate()?,
            Input::Kubernetes(target) => target.validate()?,
            Input::Docker { container, .. } => {
                if container.len() != 64 || !container.bytes().all(|b| b.is_ascii_hexdigit()) {
                    bail!("Identificativo del container Docker non valido");
                }
            }
            Input::Folder { path, .. } => {
                if !path.is_absolute() {
                    bail!("Choose an absolute folder path");
                }
            }
            Input::File {
                path,
                rotated_pattern,
            } => {
                if !path.is_absolute() || !crate::sources::file::log_name(path) {
                    bail!("Scegli il percorso assoluto di un file di log");
                }
                if let Some(pattern) = rotated_pattern {
                    if pattern.contains(['/', '\\']) || pattern == ".." {
                        bail!("Rotation patterns must be filenames in the source directory");
                    }
                    glob::Pattern::new(pattern)?;
                }
            }
            Input::Podman {
                container,
                connection,
            } => {
                if container.len() != 64 || !container.bytes().all(|c| c.is_ascii_hexdigit()) {
                    bail!("Podman sources require a complete container ID; select one with 'podman-list'");
                }
                if connection.as_ref().is_some_and(|c| {
                    c.is_empty() || c.starts_with('-') || c.chars().any(char::is_control)
                }) {
                    bail!("Invalid Podman connection name");
                }
            }
        }
    }
    if let Some(settings) = &config.updates {
        crate::update::validate_settings(settings)?;
    }
    Ok(())
}

fn valid_site_id(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

// Prefer a valid original name. For a free-form name, use a stable identifier
// that cannot merge two projects whose names differ only by punctuation.
fn create_named_site(sites: &mut BTreeMap<String, Site>, name: &str) -> String {
    let mut id = name.to_owned();
    if !valid_site_id(&id) || sites.contains_key(&id) {
        let base = format!("site-{}", &crate::util::digest(name.as_bytes())[..16]);
        id = base.clone();
        let mut suffix = 2;
        while sites.contains_key(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
    }
    sites.insert(
        id.clone(),
        Site {
            id: id.clone(),
            name: name.into(),
            note: String::new(),
        },
    );
    id
}

pub fn assigned_site(cfg: &mut Config, value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || cfg.sites.contains_key(value) {
        return value.to_owned();
    }
    if let Some(site) = cfg.sites.values().find(|s| s.name == value) {
        return site.id.clone();
    }
    create_named_site(&mut cfg.sites, value)
}

// Use the same reader for the live configuration, imports and old snapshots.
// Snapshot verification never modifies its input.
pub fn decode(mut value: Value) -> Result<Config> {
    if value["version"].as_u64() == Some(1) {
        let mut sites = Config::default().sites;
        let sources = value["sources"]
            .as_array_mut()
            .context("Elenco sorgenti mancante")?;
        let names: BTreeSet<_> = sources
            .iter()
            .filter_map(|s| s["site"].as_str())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        let mut ids = BTreeMap::new();
        for name in names
            .iter()
            .filter(|n| valid_site_id(n))
            .chain(names.iter().filter(|n| !valid_site_id(n)))
        {
            ids.insert(name.clone(), create_named_site(&mut sites, name));
        }
        for s in sources {
            let fields = s.as_object_mut().context("Sorgente non valida")?;
            if let Some(days) = fields.remove("retention_days") {
                if fields.get("retention").is_none_or(Value::is_null) {
                    let days = serde_json::from_value(days)?;
                    fields.insert(
                        "retention".into(),
                        serde_json::to_value(Retention::days(days))?,
                    );
                }
            }
            let name = match fields.get("site") {
                Some(site) => site.as_str().context("Nome del sito non valido")?,
                None => "",
            };
            let site = if !name.is_empty() {
                &ids[name]
            } else if fields.get("kind").and_then(Value::as_str) == Some("journal") {
                SERVER_SITE
            } else {
                ""
            };
            fields.insert("site".into(), Value::from(site));
        }
        value["sites"] = serde_json::to_value(sites)?;
        value["version"] = Value::from(2);
    }
    let config: Config = serde_json::from_value(value)?;
    validate(&config)?;
    Ok(config)
}

fn read_config(dir: &Path) -> Result<Option<Vec<u8>>> {
    let path = dir.join("config.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_CONFIG_BYTES,
        "Invalid or oversized configuration file"
    );
    Ok(Some(fs::read(path)?))
}

pub const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

fn write_config(dir: &Path, config: &Config) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(config)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG_BYTES,
        "Configuration file is too large"
    );
    atomic_write(&dir.join("config.json"), &bytes)
}

fn backup_before_migration(dir: &Path, bytes: &[u8]) -> Result<()> {
    let day = chrono::Utc::now().format("%Y%m%d");
    let name = format!("config.json.bak-{day}");
    for n in 0.. {
        let path = if n == 0 {
            dir.join(&name)
        } else {
            dir.join(format!("{name}-{n}"))
        };
        match fs::symlink_metadata(&path) {
            Ok(meta)
                if meta.is_file()
                    && meta.len() == bytes.len() as u64
                    && fs::read(&path)? == bytes =>
            {
                return Ok(())
            }
            Ok(_) => continue,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return atomic_write(&path, bytes)
            }
            Err(err) => return Err(err.into()),
        }
    }
    unreachable!()
}

pub(crate) fn load_locked(dir: &Path, _lock: &Lock) -> Result<Config> {
    let Some(bytes) = read_config(dir)? else {
        return Ok(Config::default());
    };
    let value: Value = serde_json::from_slice(&bytes)?;
    let migrating = value["version"].as_u64() == Some(1);
    let config = decode(value)?;
    if migrating {
        backup_before_migration(dir, &bytes)?;
        write_config(dir, &config)?;
    }
    Ok(config)
}

pub fn load(dir: &Path) -> Result<Config> {
    let Some(bytes) = read_config(dir)? else {
        return Ok(Config::default());
    };
    #[derive(serde::Deserialize)]
    struct Version {
        version: u32,
    }
    if serde_json::from_slice::<Version>(&bytes)?.version == 1 {
        let lock = Lock::acquire(&dir.join("config.lock"))?;
        return load_locked(dir, &lock);
    }
    let config: Config = serde_json::from_slice(&bytes).context("Cannot read config.json")?;
    validate(&config)?;
    Ok(config)
}

pub fn edit<T>(dir: &Path, change: impl FnOnce(&mut Config) -> Result<T>) -> Result<T> {
    private_dir(dir)?;
    let lock = Lock::acquire(&dir.join("config.lock"))?;
    let mut config = load_locked(dir, &lock)?;
    let previous_archives = config
        .sources
        .iter()
        .map(|s| (s.id.clone(), s.archive_path(dir)))
        .collect::<Vec<_>>();
    let result = change(&mut config)?;
    validate(&config)?;
    for (id, archive) in previous_archives {
        if archive.exists() && !config.sources.iter().any(|s| s.id == id) {
            bail!("Source {id} has an archive. Pause it instead of removing its retention policy");
        }
    }
    write_config(dir, &config)?;
    Ok(result)
}

/// L'identificatore di un sito indicato per identificatore o per nome.
fn site_id(cfg: &Config, value: &str) -> Result<String> {
    let value = value.trim();
    if cfg.sites.contains_key(value) {
        return Ok(value.to_owned());
    }
    cfg.sites
        .values()
        .find(|s| s.name == value)
        .map(|s| s.id.clone())
        .with_context(|| format!("Sito sconosciuto: {value}"))
}

/// Cambia il nome di un sito. I log restano collegati: cambia solo il nome mostrato.
pub fn rename_site(dir: &Path, site: &str, name: &str) -> Result<()> {
    let name = name.trim();
    ensure!(!name.is_empty(), "Indica il nuovo nome");
    edit(dir, |cfg| {
        let id = site_id(cfg, site)?;
        ensure!(id != SERVER_SITE, "«Questo server» non si rinomina");
        ensure!(
            !cfg.sites.values().any(|s| s.id != id && s.name == name),
            "Esiste già un sito con questo nome"
        );
        cfg.sites.get_mut(&id).context("Sito sconosciuto")?.name = name.to_owned();
        Ok(())
    })
}

/// I log di un sito, per identificatore o nome.
pub fn site_sources(dir: &Path, site: &str) -> Result<(String, Vec<String>)> {
    let cfg = load(dir)?;
    let id = site_id(&cfg, site)?;
    let logs = cfg
        .sources
        .iter()
        .filter(|s| s.site == id)
        .map(|s| s.id.clone())
        .collect();
    Ok((id, logs))
}

/// Toglie un sito vuoto. Con `with_logs` toglie prima i suoi log e il loro storico.
pub fn remove_site(dir: &Path, site: &str, with_logs: bool) -> Result<Vec<String>> {
    let (id, logs) = site_sources(dir, site)?;
    ensure!(id != SERVER_SITE, "«Questo server» non si elimina");
    ensure!(
        logs.is_empty() || with_logs,
        "Il sito ha {} log: spostali in un altro sito, oppure eliminali insieme al sito (cancella anche il loro storico)",
        logs.len()
    );
    // Prima si ferma la raccolta, poi si toglie ogni log con il suo storico.
    edit(dir, |cfg| {
        for s in cfg.sources.iter_mut().filter(|s| s.site == id) {
            s.enabled = false;
        }
        Ok(())
    })?;
    for log in &logs {
        crate::management::remove(dir, log)?;
    }
    edit(dir, |cfg| {
        cfg.sites.remove(&id);
        Ok(())
    })?;
    Ok(logs)
}

pub fn add_site(dir: &Path, name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty(), "Indica il nome del sito");
    edit(dir, |cfg| {
        ensure!(
            !cfg.sites.contains_key(name) && !cfg.sites.values().any(|site| site.name == name),
            "Esiste già un sito con questo nome"
        );
        Ok(create_named_site(&mut cfg.sites, name))
    })
}

pub fn add(dir: &Path, mut source: Source) -> Result<String> {
    edit(dir, |config| {
        if config.sources.iter().any(|s| s.id == source.id) {
            bail!("A source with this name already exists");
        }
        source.site = assigned_site(config, &source.site);
        if source.site.is_empty() && matches!(source.input, Input::Journal(_)) {
            source.site = SERVER_SITE.into();
        }
        let site = source.site.clone();
        config.sources.push(source);
        Ok(site)
    })
}

pub fn assign(dir: &Path, source: &str, value: &str) -> Result<()> {
    edit(dir, |cfg| {
        let id = assigned_site(cfg, value);
        cfg.sources
            .iter_mut()
            .find(|s| s.id == source)
            .context("Sorgente inesistente")?
            .site = id;
        Ok(())
    })
}

pub fn source(dir: &Path, id: &str) -> Result<Source> {
    load(dir)?
        .sources
        .into_iter()
        .find(|s| s.id == id)
        .context("Source not found")
}

pub fn archive_path(dir: &Path, id: &str) -> PathBuf {
    load(dir)
        .ok()
        .and_then(|c| c.sources.into_iter().find(|s| s.id == id))
        .map(|s| s.archive_path(dir))
        .unwrap_or_else(|| dir.join("archives").join(format!("{id}.db")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrates_sites_and_calendar_periods_without_rewriting_archives() -> Result<()> {
        let data = tempfile::tempdir()?;
        let dir = data.path();
        let mut sources = Vec::new();
        for (index, name) in ["example.com", "My project", "My project", "server", ""]
            .iter()
            .enumerate()
        {
            sources.push(json!({
                "id": format!("log{index}"), "site": name, "kind": "folder",
                "path": "/var/log/myapp", "log_kind": "generic",
                "retention_days": 42, "enabled": true
            }));
        }
        sources[1]["retention"] = json!({"unit":"months", "amount":2});
        sources.push(json!({"id":"journal", "kind":"journal", "retention_days":7, "enabled":true}));
        let original =
            serde_json::to_vec(&json!({"version":1,"poll_seconds":5,"sources":sources}))?;
        fs::write(dir.join("config.json"), &original)?;
        fs::create_dir(dir.join("archives"))?;
        let archive = dir.join("archives/log0.db");
        fs::write(&archive, b"existing archive")?;

        let cfg = load(dir)?;
        assert_eq!(cfg.sites.len(), 4);
        assert_eq!(cfg.sources[0].site, "example.com");
        assert_eq!(cfg.sources[1].site, cfg.sources[2].site);
        assert_eq!(cfg.sites[&cfg.sources[1].site].name, "My project");
        assert_ne!(cfg.sources[3].site, SERVER_SITE);
        assert_eq!(cfg.sites[&cfg.sources[3].site].name, "server");
        assert_eq!(cfg.sources[4].site, "");
        assert_eq!(cfg.sources[5].site, SERVER_SITE);
        assert_eq!(cfg.sources[0].retention, Retention::days(42));
        assert_eq!(
            cfg.sources[1].retention,
            Retention {
                amount: 2,
                unit: crate::model::RetentionUnit::Months
            }
        );
        let updated = fs::read(dir.join("config.json"))?;
        let stored: Value = serde_json::from_slice(&updated)?;
        assert_eq!(stored["sources"][1]["log_kind"], "application");
        assert!(stored["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s.get("retention_days").is_none()));
        assert_eq!(fs::read(archive)?, b"existing archive");

        assert_eq!(load(dir)?.sites, cfg.sites);
        assert_eq!(fs::read(dir.join("config.json"))?, updated);
        let backups: Vec<_> = fs::read_dir(dir)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|f| {
                f.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.bak-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(backups[0].path())?, original);
        Ok(())
    }

    #[test]
    fn invalid_configs_and_failed_assignments_leave_the_original_intact() -> Result<()> {
        let data = tempfile::tempdir()?;
        let dir = data.path();
        let path = dir.join("config.json");
        let invalid = br#"{"version":1,"poll_seconds":0,"sources":[]}"#;
        fs::write(&path, invalid)?;
        assert!(load(dir).is_err());
        assert_eq!(fs::read(&path)?, invalid);
        fs::write(&path, serde_json::to_vec(&Config::default())?)?;
        let original = fs::read(&path)?;
        assert!(assign(dir, "missing", "New site").is_err());
        assert!(edit(dir, |cfg| {
            cfg.sites.remove(SERVER_SITE);
            Ok(())
        })
        .is_err());
        assert_eq!(fs::read(&path)?, original);
        Ok(())
    }
}
