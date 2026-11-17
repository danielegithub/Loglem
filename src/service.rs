//! Servizio automatico: Loglem parte da solo e si riavvia se si ferma.
//! Linux: servizio utente systemd. macOS: agente launchd (parte all'accesso dell'utente).
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
/// Nome dell'agente launchd su macOS.
const LABEL: &str = "com.loglem.service";

#[derive(Debug, Serialize, Deserialize)]
pub struct Installation {
    pub executable: PathBuf,
    pub data_dir: PathBuf,
}

fn supported() -> Result<()> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        bail!("Il servizio automatico è disponibile su Linux (systemd) e macOS (launchd)");
    }
    Ok(())
}

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").context("HOME non disponibile")?,
    ))
}

fn path_text(path: &Path) -> Result<&str> {
    let value = path
        .to_str()
        .context("I percorsi del servizio devono essere UTF-8")?;
    if value.chars().any(char::is_control) {
        bail!("Caratteri di controllo nel percorso del servizio");
    }
    Ok(value)
}

/// Percorso tra virgolette per systemd (`%` va raddoppiato).
fn quoted(path: &Path) -> Result<String> {
    Ok(format!(
        "\"{}\"",
        path_text(path)?
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}

/// Testo per un file plist XML.
fn xml(path: &Path) -> Result<String> {
    Ok(path_text(path)?
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;"))
}

pub fn install(dir: &Path, start: bool) -> Result<()> {
    supported()?;
    util::private_dir(dir)?;
    let data_dir = fs::canonicalize(dir)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    if cfg!(target_os = "macos") {
        write_launchd_agent(&executable, &data_dir)?;
    } else {
        write_systemd_unit(&executable, &data_dir)?;
        systemctl(&["daemon-reload"])?;
        systemctl(&["enable", UNIT])?;
    }
    util::atomic_write(
        &dir.join("installation.json"),
        &serde_json::to_vec_pretty(&Installation {
            executable,
            data_dir,
        })?,
    )?;
    if start {
        self::start()?;
    }
    Ok(())
}

fn write_systemd_unit(executable: &Path, data_dir: &Path) -> Result<()> {
    let unit_dir = home()?.join(".config/systemd/user");
    fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join(UNIT);
    if unit_path.exists() && !fs::read_to_string(&unit_path)?.starts_with(MARKER) {
        bail!("Esiste già un loglem.service non gestito da Loglem: non lo sovrascrivo");
    }
    let unit = format!(
        "{MARKER}\n\
         [Unit]\n\
         Description=Loglem: raccolta dei log e interfaccia web\n\
         StartLimitIntervalSec=300\n\
         StartLimitBurst=10\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} --data-dir {} serve\n\
         Restart=always\n\
         RestartSec=5\n\
         TimeoutStopSec=150\n\
         UMask=0077\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        quoted(executable)?,
        quoted(data_dir)?,
    );
    // Restart=always con un limite: se fallisce 10 volte in 5 minuti systemd si ferma.
    util::atomic_write(&unit_path, unit.as_bytes())
}

fn agent_path() -> Result<PathBuf> {
    Ok(home()?
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn write_launchd_agent(executable: &Path, data_dir: &Path) -> Result<()> {
    let path = agent_path()?;
    fs::create_dir_all(path.parent().context("cartella mancante")?)?;
    if path.exists() && !fs::read_to_string(&path)?.contains(MARKER) {
        bail!(
            "Esiste già {} non gestito da Loglem: non lo sovrascrivo",
            path.display()
        );
    }
    let log = data_dir.join("diagnostics/service.log");
    // KeepAlive: launchd lo riavvia se si ferma; ThrottleInterval: non più di una volta ogni 10 s.
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- {MARKER} -->
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>--data-dir</string>
		<string>{}</string>
		<string>serve</string>
	</array>
	<key>RunAtLoad</key><true/>
	<key>KeepAlive</key><true/>
	<key>ThrottleInterval</key><integer>10</integer>
	<key>ExitTimeOut</key><integer>150</integer>
	<key>Umask</key><integer>63</integer>
	<key>StandardOutPath</key><string>{}</string>
	<key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(executable)?,
        xml(data_dir)?,
        xml(&log)?,
        xml(&log)?,
    );
    util::atomic_write(&path, plist.as_bytes())
}

fn systemctl(args: &[&str]) -> Result<()> {
    process::output(
        Command::new("systemctl").arg("--user").args(args),
        Duration::from_secs(160),
    )?;
    Ok(())
}

/// Dominio launchd dell'utente: `gui/<uid>`.
fn launchd_domain() -> String {
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = 0;
    format!("gui/{uid}")
}

fn launchctl(args: &[&str]) -> Result<std::process::ExitStatus> {
    process::stream(
        Command::new("launchctl").args(args),
        Duration::from_secs(160),
        |_, _| Ok(()),
    )
}

pub fn start() -> Result<()> {
    supported()?;
    if cfg!(target_os = "macos") {
        let path = agent_path()?;
        let path = path_text(&path)?;
        // Se è già caricato, bootstrap fallisce: allora basta riavviarlo.
        if !launchctl(&["bootstrap", &launchd_domain(), path])?.success() {
            launchctl(&["kickstart", "-k", &format!("{}/{LABEL}", launchd_domain())])?;
        }
        Ok(())
    } else {
        systemctl(&["start", UNIT])
    }
}

pub fn stop() -> Result<()> {
    supported()?;
    if cfg!(target_os = "macos") {
        launchctl(&["bootout", &format!("{}/{LABEL}", launchd_domain())])?;
        Ok(())
    } else {
        systemctl(&["stop", UNIT])
    }
}

pub fn is_active() -> Result<bool> {
    supported()?;
    if cfg!(target_os = "macos") {
        let mut running = false;
        process::stream(
            Command::new("launchctl").args(["print", &format!("{}/{LABEL}", launchd_domain())]),
            Duration::from_secs(10),
            |_, line| {
                running |= String::from_utf8_lossy(&line).trim() == "state = running";
                Ok(())
            },
        )?;
        return Ok(running);
    }
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
