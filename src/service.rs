use crate::{process, util};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub const UNIT: &str = "loglem.service";
const MARKER: &str = "# Managed by Loglem";

#[derive(Debug, Serialize, Deserialize)]
pub struct Installation {
    pub executable: PathBuf,
    pub data_dir: PathBuf,
}

fn linux() -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("Service installation is supported on Linux with systemd only");
    }
    Ok(())
}

fn quoted(path: &Path) -> Result<String> {
    let value = path.to_str().context("Service paths must be UTF-8")?;
    if value.chars().any(char::is_control) {
        bail!("Control characters in service path");
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}

pub fn install(dir: &Path, start: bool) -> Result<()> {
    linux()?;
    util::private_dir(dir)?;
    let data_dir = fs::canonicalize(dir)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let user_home = std::env::var_os("HOME").context("HOME is unavailable")?;
    let unit_dir = PathBuf::from(user_home).join(".config/systemd/user");
    fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join(UNIT);
    if unit_path.exists() && !fs::read_to_string(&unit_path)?.starts_with(MARKER) {
        bail!("An existing loglem.service is not managed by Loglem; not overwritten");
    }
    let unit = format!(
        concat!(
            "{MARKER}\n",
            "[Unit]\n",
            "Description=Loglem: raccolta dei log e interfaccia web\n",
            "\n",
            "[Service]\n",
            "Type=simple\n",
            "ExecStart={} --data-dir {} serve\n",
            "Restart=on-failure\n",
            "RestartSec=5\n",
            "TimeoutStopSec=150\n",
            "UMask=0077\n",
            "\n",
            "[Install]\n",
            "WantedBy=default.target\n"
        ),
        quoted(&executable)?,
        quoted(&data_dir)?,
        MARKER = MARKER
    );
    util::atomic_write(&unit_path, unit.as_bytes())?;
    run(&["daemon-reload"])?;
    run(&["enable", UNIT])?;
    util::atomic_write(
        &dir.join("installation.json"),
        &serde_json::to_vec_pretty(&Installation {
            executable,
            data_dir,
        })?,
    )?;
    if start {
        run(&["start", UNIT])?;
    }
    Ok(())
}

pub fn run(args: &[&str]) -> Result<()> {
    linux()?;
    process::output(
        Command::new("systemctl").arg("--user").args(args),
        Duration::from_secs(160),
    )?;
    Ok(())
}

pub fn is_active() -> Result<bool> {
    linux()?;
    let status = process::stream(
        Command::new("systemctl").args(["--user", "is-active", "--quiet", UNIT]),
        Duration::from_secs(10),
        |_, _| Ok(()),
    )?;
    Ok(status.success())
}

pub fn installation(dir: &Path) -> Result<Installation> {
    let install: Installation = serde_json::from_slice(
        &fs::read(dir.join("installation.json"))
            .context("Install the user service first with 'loglem service install --start'")?,
    )?;
    if install.data_dir != fs::canonicalize(dir)?
        || install.executable != std::env::current_exe()?.canonicalize()?
    {
        bail!("This executable/data directory does not match the installed service");
    }
    Ok(install)
}
