use crate::{
    model::{Config, Input, Source},
    util::{atomic_write, private_dir, Lock},
};
use anyhow::{bail, Context, Result};
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
    if config.version != 1 {
        bail!("Unsupported configuration version {}", config.version);
    }
    config.access.validate()?;
    if !(1..=3600).contains(&config.poll_seconds) {
        bail!("Polling must be between 1 and 3600 seconds");
    }
    let mut ids = std::collections::HashSet::new();
    for source in &config.sources {
        validate_id(&source.id)?;
        if !ids.insert(&source.id) {
            bail!("Duplicate source {}", source.id);
        }
        if !(1..=36500).contains(&source.retention_days) {
            bail!("Retention must be between 1 and 36500 days");
        }
        source.period().validate()?;
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
                Input::Folder { path } if path == completed => {
                    bail!("La cartella dei file conclusi deve essere diversa dalla cartella attiva")
                }
                Input::File { path, .. } if path.parent() == Some(completed.as_path()) => {
                    bail!("La cartella dei file conclusi deve essere separata dal file attivo")
                }
                Input::Podman { .. } | Input::Journal(_) => {
                    bail!("La pulizia dei file conclusi vale solo per sorgenti file")
                }
                _ => {}
            }
        }
        match &source.input {
            Input::Journal(filter) => filter.validate()?,
            Input::Folder { path } => {
                if !path.is_absolute() {
                    bail!("Choose an absolute folder path");
                }
            }
            Input::File {
                path,
                rotated_pattern,
            } => {
                if !path.is_absolute() || path.extension().and_then(|x| x.to_str()) != Some("log") {
                    bail!("Choose an absolute path to a .log file");
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

pub fn load(dir: &Path) -> Result<Config> {
    let path = dir.join("config.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        bail!("Invalid or oversized configuration file");
    }
    let config: Config =
        serde_json::from_slice(&fs::read(&path)?).context("Cannot read config.json")?;
    validate(&config)?;
    Ok(config)
}

pub fn edit(dir: &Path, change: impl FnOnce(&mut Config) -> Result<()>) -> Result<()> {
    private_dir(dir)?;
    let _lock = Lock::acquire(&dir.join("config.lock"))?;
    let mut config = load(dir)?;
    let previous_ids = config
        .sources
        .iter()
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    change(&mut config)?;
    validate(&config)?;
    for id in previous_ids {
        if archive_path(dir, &id).exists() && !config.sources.iter().any(|s| s.id == id) {
            bail!("Source {id} has an archive. Pause it instead of removing its retention policy");
        }
    }
    atomic_write(
        &dir.join("config.json"),
        &serde_json::to_vec_pretty(&config)?,
    )
}

pub fn add(dir: &Path, source: Source) -> Result<()> {
    edit(dir, |config| {
        if config.sources.iter().any(|s| s.id == source.id) {
            bail!("A source with this name already exists");
        }
        config.sources.push(source);
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
