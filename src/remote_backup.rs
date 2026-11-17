//! Backup verificato, cifrato con la chiave pubblica OpenPGP e trasferito via rsync/SSH.
use crate::{backup, config, process, util};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::Command, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub hours: u32,
    pub recipient: String,
    pub destination: String,
    #[serde(default)]
    pub classes_only: bool,
}
impl Schedule {
    pub fn due(&self, state: &State) -> bool {
        let now = chrono::Utc::now().timestamp();
        state
            .success
            .is_none_or(|at| now - at >= i64::from(self.hours) * 3600)
            && now - state.attempt >= 300
    }
    pub fn validate(&self) -> Result<()> {
        ensure!((1..=8760).contains(&self.hours), "Intervallo non valido");
        ensure!(
            matches!(self.recipient.len(), 40 | 64)
                && self.recipient.bytes().all(|c| c.is_ascii_hexdigit()),
            "Indica l’impronta completa della chiave pubblica GPG"
        );
        let (host, path) = self
            .destination
            .split_once(':')
            .context("Destinazione richiesta: utente@host:/cartella")?;
        ensure!(
            !host.is_empty()
                && !host.starts_with('-')
                && host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"@.-_".contains(&b))
                && path.starts_with('/')
                && path
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/-._".contains(&b)),
            "Destinazione SSH non valida"
        );
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub attempt: i64,
    pub success: Option<i64>,
    pub message: String,
}
pub fn status(dir: &Path) -> State {
    fs::read(dir.join("backup-status.json"))
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default()
}
fn save(dir: &Path, state: &State) -> Result<()> {
    util::atomic_write(&dir.join("backup-status.json"), &serde_json::to_vec(state)?)
}

pub fn run(dir: &Path, manual: bool) -> Result<()> {
    let Some(plan) = config::load(dir)?.rules.remote else {
        anyhow::ensure!(!manual, "Configura prima la destinazione del backup");
        return Ok(());
    };
    let now = chrono::Utc::now().timestamp();
    let due = |s: &State| manual || plan.due(s);
    if !due(&status(dir)) {
        return Ok(());
    }
    let _lock = util::Lock::acquire(&dir.join("backup-transfer.lock"))?;
    let mut state = status(dir);
    if !due(&state) {
        return Ok(());
    }
    state.attempt = now;
    state.message = "Backup in corso".into();
    save(dir, &state)?;
    match transfer(dir, &plan) {
        Ok(file) => {
            state.success = Some(chrono::Utc::now().timestamp());
            state.message = format!("Verificato: {file}");
            save(dir, &state)
        }
        Err(e) => {
            state.message = format!("Backup fallito: {e:#}");
            save(dir, &state)?;
            Err(e)
        }
    }
}
fn transfer(dir: &Path, plan: &Schedule) -> Result<String> {
    let staging = tempfile::Builder::new()
        .prefix(".backup-")
        .tempdir_in(dir)?;
    let root = staging.path();
    let snapshot = root.join("snapshot");
    backup::create_selected(dir, &snapshot, plan.classes_only)?;
    backup::verify(&snapshot)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%fZ");
    let filename = format!("loglem-{stamp}.tar.gz.gpg");
    let packed = root.join("snapshot.tar.gz");
    let encrypted = root.join(&filename);
    let timeout = Duration::from_secs(3600);
    process::output(
        Command::new("tar")
            .arg("-czf")
            .arg(&packed)
            .arg("-C")
            .arg(&snapshot)
            .arg("."),
        timeout,
    )?;
    process::output(
        Command::new("gpg")
            .args([
                "--batch",
                "--no-auto-key-locate",
                "--trust-model",
                "always",
                "--recipient",
                &plan.recipient,
                "--output",
            ])
            .arg(&encrypted)
            .arg("--encrypt")
            .arg(&packed),
        timeout,
    )?;
    fs::remove_file(packed)?;
    fs::remove_dir_all(snapshot)?;
    let remote = format!("{}/{}", plan.destination.trim_end_matches('/'), filename);
    let ssh = "ssh -o BatchMode=yes -o StrictHostKeyChecking=yes";
    process::output(
        Command::new("rsync")
            .args(["-e", ssh, "--"])
            .arg(&encrypted)
            .arg(&remote),
        timeout,
    )?;
    let returned = root.join("received.gpg");
    process::output(
        Command::new("rsync")
            .args(["-e", ssh, "--"])
            .arg(&remote)
            .arg(&returned),
        timeout,
    )?;
    ensure!(
        util::file_digest(&returned)? == util::file_digest(&encrypted)?,
        "Il file remoto non coincide con il backup inviato"
    );
    Ok(remote)
}
