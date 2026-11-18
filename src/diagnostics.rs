//! Persistent diagnostics for Loglem itself; never copies application log records.
use anyhow::Result;
use serde_json::{json, Value};
use std::{
    backtrace::Backtrace,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());
const MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn init(dir: &Path) -> Result<()> {
    crate::util::private_dir(&dir.join("diagnostics"))?;
    let _ = DIRECTORY.set(dir.to_owned());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error("runtime", "panic", None, &anyhow::anyhow!(info.to_string()));
        previous(info);
    }));
    Ok(())
}

pub fn error(component: &str, operation: &str, source: Option<&str>, error: &anyhow::Error) {
    if let Some(dir) = DIRECTORY.get() {
        if let Err(failure) = record(dir, component, operation, source, error) {
            eprintln!("Cannot write Loglem diagnostic log: {failure:#}");
        }
    }
}

pub fn record(
    dir: &Path,
    component: &str,
    operation: &str,
    source: Option<&str>,
    error: &anyhow::Error,
) -> Result<()> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let folder = dir.join("diagnostics");
    crate::util::private_dir(&folder)?;
    let path = folder.join("loglem.log");
    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) >= MAX_BYTES {
        let _ = fs::remove_file(folder.join("loglem.2.log"));
        if folder.join("loglem.1.log").exists() {
            fs::rename(folder.join("loglem.1.log"), folder.join("loglem.2.log"))?;
        }
        fs::rename(&path, folder.join("loglem.1.log"))?;
    }
    let truncate = |s: String| s.chars().take(32_768).collect::<String>();
    let event = json!({"timestamp":crate::timestamps::format_ns(crate::timestamps::now_ns()),"level":"error","version":crate::VERSION,"pid":std::process::id(),"component":component,"operation":operation,"source":source,"message":truncate(error.to_string()),"causes":error.chain().skip(1).take(16).map(|e|truncate(e.to_string())).collect::<Vec<_>>(),"backtrace":truncate(Backtrace::capture().to_string())});
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(&mut file, &event)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

pub fn recent(dir: &Path) -> Result<Value> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut events = std::collections::VecDeque::new();
    for name in ["loglem.2.log", "loglem.1.log", "loglem.log"] {
        let path = dir.join("diagnostics").join(name);
        if !path.exists() {
            continue;
        }
        for line in BufReader::new(fs::File::open(path)?).lines() {
            if let Ok(value) = serde_json::from_str::<Value>(&line?) {
                events.push_back(value);
                if events.len() > 40 {
                    events.pop_front();
                }
            }
        }
    }
    let events = events.into_iter().rev().collect::<Vec<_>>();
    Ok(json!({"path":dir.join("diagnostics/loglem.log"),"events":events}))
}
