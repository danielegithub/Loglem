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

/// Cosa è stato installato, da mostrare a chi ha lanciato il comando.
#[derive(Debug, Clone)]
pub struct Report {
    pub file: PathBuf,
    pub manager: &'static str,
    pub check_minutes: u32,
}

impl Report {
    pub fn text(&self) -> String {
        format!(
            "Servizio installato ({}): {}\n\
             Controllo automatico: acceso, ogni {} minuti, dentro il servizio (non serve cron).\n\
             Per toglierlo: loglem service uninstall",
            self.manager,
            self.file.display(),
            self.check_minutes
        )
    }
}

pub fn install(dir: &Path, start: bool) -> Result<Report> {
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
    // Il controllo automatico gira dentro il servizio: installarlo lo accende.
    crate::monitor::configure(dir, Some(true), None)?;
    if start {
        self::start()?;
    }
    Ok(Report {
        file: if cfg!(target_os = "macos") {
            agent_path()?
        } else {
            home()?.join(".config/systemd/user").join(UNIT)
        },
        manager: if cfg!(target_os = "macos") {
            "launchd"
        } else {
            "systemd"
        },
        check_minutes: crate::config::load(dir)?.alerts.monitor.minutes,
    })
}

/// Toglie il servizio: lo ferma, lo disabilita e cancella il file che Loglem aveva scritto.
/// I dati e le impostazioni restano.
pub fn uninstall(dir: &Path) -> Result<()> {
    supported()?;
    if cfg!(target_os = "macos") {
        let path = agent_path()?;
        if path.exists() {
            if !fs::read_to_string(&path)?.contains(MARKER) {
                bail!(
                    "{} non è stato scritto da Loglem: non lo tolgo",
                    path.display()
                );
            }
            let _ = launchctl(&["bootout", &format!("{}/{LABEL}", launchd_domain())]);
            fs::remove_file(&path)?;
        }
    } else {
        let path = home()?.join(".config/systemd/user").join(UNIT);
        if path.exists() {
            if !fs::read_to_string(&path)?.starts_with(MARKER) {
                bail!(
                    "{} non è stato scritto da Loglem: non lo tolgo",
                    path.display()
                );
            }
            let _ = systemctl(&["disable", "--now", UNIT]);
            fs::remove_file(&path)?;
            let _ = systemctl(&["daemon-reload"]);
        }
    }
    match fs::remove_file(dir.join("installation.json")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Lo stato del servizio com'è adesso, per la pagina «Sistema».
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Details {
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
    /// Si è fermato con un errore o ha finito i riavvii automatici: serve un intervento.
    pub failed: bool,
    pub since: Option<String>,
    pub restarts: Option<u32>,
    pub main_pid: Option<u32>,
    /// Questa pagina è servita proprio dal servizio (e non da un `loglem web` avviato a mano).
    pub this_process: bool,
    pub file: Option<String>,
    pub manager: &'static str,
}

fn property<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .map(str::trim)
}

/// «Tue 2026-11-17 10:00:00 UTC» → data in formato standard.
fn systemd_time(text: &str) -> Option<String> {
    let rest = text.split_once(' ')?.1.trim().strip_suffix(" UTC")?;
    let time = chrono::NaiveDateTime::parse_from_str(rest, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(crate::timestamps::format_ns(
        time.and_utc().timestamp_nanos_opt()?,
    ))
}

fn parse_systemd(text: &str) -> Details {
    let active = property(text, "ActiveState") == Some("active");
    let pid = property(text, "MainPID")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|p| *p > 0);
    Details {
        supported: true,
        active,
        failed: property(text, "ActiveState") == Some("failed"),
        since: active
            .then(|| property(text, "ActiveEnterTimestamp").and_then(systemd_time))
            .flatten(),
        restarts: property(text, "NRestarts").and_then(|v| v.parse().ok()),
        main_pid: pid,
        manager: "systemd",
        ..Details::default()
    }
}

fn parse_launchd(text: &str) -> Details {
    let value = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key)?.strip_prefix(" = "))
            .map(str::trim)
    };
    let active = value("state") == Some("running");
    let failed = !active
        && value("last exit code")
            .and_then(|v| v.parse::<i32>().ok())
            .is_some_and(|code| code != 0);
    Details {
        supported: true,
        active,
        failed,
        restarts: value("runs")
            .and_then(|v| v.parse::<u32>().ok())
            .map(|n| n.saturating_sub(1)),
        main_pid: value("pid").and_then(|v| v.parse().ok()),
        manager: "launchd",
        ..Details::default()
    }
}

pub fn details() -> Details {
    if supported().is_err() {
        return Details::default();
    }
    let mut found;
    if cfg!(target_os = "macos") {
        let Ok(file) = agent_path() else {
            return Details::default();
        };
        let installed = fs::read_to_string(&file).is_ok_and(|t| t.contains(MARKER));
        let output = process::output(
            Command::new("launchctl").args(["print", &format!("{}/{LABEL}", launchd_domain())]),
            Duration::from_secs(10),
        )
        .map(|o| String::from_utf8_lossy(&o).into_owned())
        .unwrap_or_default();
        found = parse_launchd(&output);
        found.installed = installed;
        found.file = installed.then(|| file.display().to_string());
    } else {
        let Ok(home) = home() else {
            return Details::default();
        };
        let file = home.join(".config/systemd/user").join(UNIT);
        let installed = fs::read_to_string(&file).is_ok_and(|t| t.starts_with(MARKER));
        let show = |extra: &[&str]| {
            process::output(
                Command::new("systemctl")
                    .arg("--user")
                    .arg("show")
                    .arg(UNIT)
                    .args(extra)
                    .arg("--property=ActiveState,NRestarts,MainPID,ActiveEnterTimestamp"),
                Duration::from_secs(10),
            )
            .map(|o| String::from_utf8_lossy(&o).into_owned())
        };
        // `--timestamp=utc` esiste da systemd 247: con uno più vecchio si perde solo «da quando».
        let output = show(&["--timestamp=utc"])
            .or_else(|_| show(&[]))
            .unwrap_or_default();
        found = parse_systemd(&output);
        found.installed = installed;
        found.file = installed.then(|| file.display().to_string());
    }
    found.this_process = found.main_pid == Some(std::process::id());
    found
}

/// Riavvia il servizio. Se è questo stesso processo, la chiamata non torna.
pub fn restart() -> Result<()> {
    supported()?;
    if cfg!(target_os = "macos") {
        launchctl(&["kickstart", "-k", &format!("{}/{LABEL}", launchd_domain())])?;
        Ok(())
    } else {
        systemctl(&["restart", UNIT])
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_running_systemd_service() {
        let d = parse_systemd(
            "ActiveState=active\nNRestarts=2\nMainPID=4321\nActiveEnterTimestamp=Tue 2026-11-17 10:00:00 UTC\n",
        );
        assert!(d.active && !d.failed);
        assert_eq!(d.restarts, Some(2));
        assert_eq!(d.main_pid, Some(4321));
        assert_eq!(d.since.as_deref(), Some("2026-11-17T10:00:00.000000000Z"));
    }

    #[test]
    fn a_failed_systemd_service_needs_attention_and_has_no_start_time() {
        let d = parse_systemd(
            "ActiveState=failed\nNRestarts=10\nMainPID=0\nActiveEnterTimestamp=Tue 2026-11-17 10:00:00 UTC\n",
        );
        assert!(d.failed && !d.active);
        assert_eq!(d.main_pid, None);
        assert_eq!(d.since, None);
        assert_eq!(d.restarts, Some(10));
    }

    #[test]
    fn an_old_systemd_without_utc_timestamps_still_gives_the_state() {
        let d = parse_systemd(
            "ActiveState=active\nActiveEnterTimestamp=Tue 2026-11-17 11:00:00 CET\nMainPID=7\n",
        );
        assert!(d.active);
        assert_eq!(d.since, None);
    }

    #[test]
    fn reads_launchd_state_and_restarts() {
        let running = parse_launchd(
            "\tstate = running\n\tpid = 99\n\truns = 3\n\tlast exit code = (never exited)\n",
        );
        assert!(running.active && !running.failed);
        assert_eq!(running.restarts, Some(2));
        assert_eq!(running.main_pid, Some(99));
        let broken = parse_launchd("\tstate = waiting\n\truns = 11\n\tlast exit code = 1\n");
        assert!(!broken.active && broken.failed);
    }

    #[test]
    fn the_install_report_says_what_was_installed_and_how_to_remove_it() {
        let text = Report {
            file: PathBuf::from("/home/user/.config/systemd/user/loglem.service"),
            manager: "systemd",
            check_minutes: 5,
        }
        .text();
        assert!(text.contains("loglem.service"));
        assert!(text.contains("ogni 5 minuti"));
        assert!(text.contains("non serve cron"));
        assert!(text.contains("loglem service uninstall"));
    }
}
