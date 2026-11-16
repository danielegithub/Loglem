//! Full-binary, signed updates. Logical modules do not require a public plugin ABI.
use crate::{config, model::UpdateSettings, process, service, util, VERSION};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const MAX_BINARY: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub target: String,
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub version: String,
    pub schema: u32,
    pub artifacts: Vec<Artifact>,
}

pub fn target() -> Result<&'static str> {
    if !cfg!(target_os = "linux") {
        bail!("Automatic updates currently support Linux only");
    }
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64-unknown-linux-gnu"),
        "aarch64" => Ok("aarch64-unknown-linux-gnu"),
        _ => bail!("No release target for this architecture"),
    }
}

pub fn validate_settings(settings: &UpdateSettings) -> Result<()> {
    let parts: Vec<_> = settings.repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        bail!("Repository must be owner/name");
    }
    let bytes = STANDARD
        .decode(&settings.public_key)
        .context("Invalid base64 update public key")?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Update public key must contain 32 bytes"))?;
    VerifyingKey::from_bytes(&key)?;
    Ok(())
}

pub fn settings(dir: &Path) -> Result<UpdateSettings> {
    if let Some(settings) = config::load(dir)?.updates {
        return Ok(settings);
    }
    match (
        option_env!("LOGLEM_RELEASE_REPO"),
        option_env!("LOGLEM_UPDATE_PUBLIC_KEY"),
    ) {
        (Some(repository), Some(public_key))
            if !repository.is_empty() && !public_key.is_empty() =>
        {
            let settings = UpdateSettings {
                repository: repository.into(),
                public_key: public_key.into(),
            };
            validate_settings(&settings)?;
            Ok(settings)
        }
        _ => bail!(concat!(
            "No trusted release publisher configured. Use 'update configure' with a ",
            "repository and its independently verified public key"
        )),
    }
}

pub fn verify_manifest(
    bytes: &[u8],
    signature: &[u8],
    settings: &UpdateSettings,
) -> Result<Manifest> {
    validate_settings(settings)?;
    let key: [u8; 32] = STANDARD
        .decode(&settings.public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid key"))?;
    let signature = Signature::from_slice(signature)?;
    VerifyingKey::from_bytes(&key)?
        .verify_strict(bytes, &signature)
        .context("Release signature verification failed")?;
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    if manifest.format != 1 || manifest.schema != 1 {
        bail!("This release needs a different updater or archive schema; use its installer");
    }
    let version = semver::Version::parse(&manifest.version)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("Automatic updates accept stable version tags only");
    }
    for artifact in &manifest.artifacts {
        if artifact.name != format!("loglem-{}", artifact.target)
            || artifact.target.contains(['/', '\\'])
            || artifact.size == 0
            || artifact.size > MAX_BINARY
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("Invalid release artifact metadata");
        }
    }
    Ok(manifest)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .https_only(true)
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("Loglem/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn fetch(url: &str, max: u64) -> Result<Vec<u8>> {
    let response = agent()
        .get(url)
        .call()
        .context("Cannot download release metadata")?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("Download exceeds permitted size");
    }
    Ok(bytes)
}

pub fn check(dir: &Path) -> Result<Manifest> {
    let settings = settings(dir)?;
    let base = format!(
        "https://github.com/{}/releases/latest/download",
        settings.repository
    );
    let bytes = fetch(&format!("{base}/manifest.json"), 64 * 1024)?;
    let signature = fetch(&format!("{base}/manifest.sig"), 64)?;
    verify_manifest(&bytes, &signature, &settings)
}

fn status(dir: &Path, message: &str) -> Result<()> {
    util::atomic_write(&dir.join("updates/status.txt"), message.as_bytes())
}

pub fn prepare(dir: &Path) -> Result<PathBuf> {
    service::installation(dir)?;
    if !service::is_active()? {
        bail!("Start the installed service before requesting an automatic update");
    }
    let updates = dir.join("updates");
    util::private_dir(&updates)?;
    let _lock = util::Lock::acquire(&updates.join("update.lock"))?;
    let settings = settings(dir)?;
    let base = format!(
        "https://github.com/{}/releases/latest/download",
        settings.repository
    );
    let bytes = fetch(&format!("{base}/manifest.json"), 64 * 1024)?;
    let signature = fetch(&format!("{base}/manifest.sig"), 64)?;
    let manifest = verify_manifest(&bytes, &signature, &settings)?;
    if semver::Version::parse(&manifest.version)? <= semver::Version::parse(VERSION)? {
        bail!("No newer release is available");
    }
    let artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.target == target().unwrap_or(""))
        .context("Release has no binary for this platform")?;
    let stage = updates.join(format!(
        "stage-{}-{}",
        manifest.version,
        crate::timestamps::now_ns()
    ));
    util::private_dir(&stage)?;
    util::atomic_write(&stage.join("manifest.json"), &bytes)?;
    util::atomic_write(&stage.join("manifest.sig"), &signature)?;
    status(dir, "Downloading and verifying update")?;
    let url = format!(
        "https://github.com/{}/releases/download/v{}/{}",
        settings.repository, manifest.version, artifact.name
    );
    let response = agent()
        .get(&url)
        .call()
        .context("Cannot download application")?;
    let mut file = util::private_file(&stage.join("loglem"))?;
    let downloaded = std::io::copy(
        &mut response.into_reader().take(artifact.size + 1),
        &mut file,
    )?;
    file.sync_all()?;
    if downloaded != artifact.size || util::file_digest(&stage.join("loglem"))? != artifact.sha256 {
        bail!("Downloaded executable failed verification; current version unchanged");
    }
    util::sync_dir(&stage)?;
    status(dir, "Update verified; ready to apply")?;
    Ok(stage)
}

pub fn launch(dir: &Path, stage: &Path) -> Result<()> {
    let output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("updates/updater.log"))?;
    let mut command = Command::new(std::env::current_exe()?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .arg("--data-dir")
        .arg(fs::canonicalize(dir)?)
        .arg("__apply-update")
        .arg(fs::canonicalize(stage)?)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .spawn()
        .context("Cannot launch independent updater")?;
    Ok(())
}

pub fn apply(dir: &Path, stage: &Path) -> Result<()> {
    let updates = dir.join("updates");
    let _lock = util::Lock::acquire(&updates.join("update.lock"))?;
    let result = apply_inner(dir, stage);
    if let Err(error) = &result {
        let _ = status(dir, &format!("Update failed: {error:#}"));
    }
    result
}

fn apply_inner(dir: &Path, stage: &Path) -> Result<()> {
    let install = service::installation(dir)?;
    let settings = settings(dir)?;
    let stage = fs::canonicalize(stage)?;
    if stage.parent() != Some(fs::canonicalize(dir.join("updates"))?.as_path()) {
        bail!("Invalid update staging directory");
    }
    let manifest = verify_manifest(
        &fs::read(stage.join("manifest.json"))?,
        &fs::read(stage.join("manifest.sig"))?,
        &settings,
    )?;
    if semver::Version::parse(&manifest.version)? <= semver::Version::parse(VERSION)? {
        bail!("Refusing an older or equal version");
    }
    let artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.target == target().unwrap_or(""))
        .context("Wrong update target")?;
    let binary = stage.join("loglem");
    if fs::metadata(&binary)?.len() != artifact.size
        || util::file_digest(&binary)? != artifact.sha256
    {
        bail!("Staged binary has changed");
    }
    let parent = install
        .executable
        .parent()
        .context("Invalid executable location")?;
    let pending = parent.join(format!(".loglem-update-{}", std::process::id()));
    let backup = parent.join("loglem.previous");
    {
        let mut destination = util::private_file(&pending)?;
        std::io::copy(&mut fs::File::open(&binary)?, &mut destination)?;
        destination.flush()?;
        destination.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o755))?;
    }
    // Verify the candidate before interrupting collection.
    process::output(
        Command::new(&pending).arg("self-check"),
        Duration::from_secs(10),
    )?;
    let version_output = process::output(
        Command::new(&pending).arg("--version"),
        Duration::from_secs(10),
    )?;
    if String::from_utf8(version_output)?.trim() != format!("loglem {}", manifest.version) {
        bail!("Executable version does not match signed manifest");
    }
    status(dir, "Stopping collector and installing update")?;
    service::run(&["stop", service::UNIT])?;
    let mut replaced = false;
    let replacement = (|| -> Result<()> {
        // Copy, sync, then replace atomically on the same filesystem.
        let backup_temp = parent.join(format!(".loglem-backup-{}", std::process::id()));
        fs::copy(&install.executable, &backup_temp)?;
        fs::File::open(&backup_temp)?.sync_all()?;
        fs::rename(&backup_temp, &backup)?;
        fs::rename(&pending, &install.executable)?;
        replaced = true;
        util::sync_dir(parent)?;
        service::run(&["start", service::UNIT])?;
        thread::sleep(Duration::from_secs(2));
        if !service::is_active()? {
            bail!("Updated service failed to remain active");
        }
        Ok(())
    })();
    if let Err(error) = replacement {
        let _ = service::run(&["stop", service::UNIT]);
        if replaced {
            fs::rename(&backup, &install.executable)?;
            util::sync_dir(parent)?;
        }
        service::run(&["start", service::UNIT])
            .context("Update failed and previous service could not be restarted")?;
        bail!("Previous version restored: {error:#}");
    }
    status(
        dir,
        &format!("Updated successfully to {}", manifest.version),
    )?;
    fs::remove_dir_all(&stage)?;
    Ok(())
}
