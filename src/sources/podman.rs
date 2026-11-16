use crate::{archive::Archive, model::Entry, process, timestamps, util, BATCH_SIZE};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{process::Command, time::Duration};

#[derive(Debug, Clone, Serialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub pod: String,
    pub state: String,
}

pub fn command(connection: Option<&str>) -> Command {
    let mut command = Command::new("podman");
    if let Some(connection) = connection {
        command.args(["--connection", connection]);
    }
    command
}

pub fn discover(connection: Option<&str>) -> Result<Vec<Container>> {
    let data = process::output(
        command(connection).args(["ps", "--all", "--pod", "--format=json"]),
        Duration::from_secs(20),
    )?;
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&data).context("Podman did not return a container list")?;
    rows.into_iter()
        .map(|row| {
            let field = |keys: &[&str]| -> String {
                keys.iter()
                    .find_map(|key| {
                        row.get(key).and_then(|v| {
                            v.as_str().map(str::to_owned).or_else(|| {
                                v.as_array()
                                    .and_then(|a| a.first())
                                    .and_then(|x| x.as_str())
                                    .map(str::to_owned)
                            })
                        })
                    })
                    .unwrap_or_default()
            };
            let id = field(&["Id", "ID", "id"]);
            if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("Podman returned an invalid container ID");
            }
            Ok(Container {
                id,
                name: field(&["Names", "Name", "names"]),
                pod: field(&["PodName", "Pod", "pod"]),
                state: field(&["State", "Status", "state"]),
            })
        })
        .collect()
}

fn decode(bytes: Vec<u8>, stderr: bool, store: &Archive, now: i64) -> Result<Entry> {
    let line = std::str::from_utf8(&bytes).context("Podman returned non-UTF-8 output")?;
    let (stamp, message) = line
        .split_once(' ')
        .context("Podman log line has no timestamp")?;
    let event_ns = chrono::DateTime::parse_from_rfc3339(stamp)
        .context("Podman returned a diagnostic or a log without an RFC3339 timestamp")?
        .timestamp_nanos_opt()
        .context("Timestamp out of range")?;
    let hash = util::digest(format!("{stderr}\0{event_ns}\0{message}").as_bytes());
    let occurrence = store.occurrence(&hash)?;
    Ok(Entry {
        key: format!("podman:{hash}:{occurrence}"),
        file_id: None,
        generation: 0,
        offset: 0,
        event_ns,
        collected_ns: now,
        timestamp_origin: "event",
        raw: bytes.clone(),
        message: message.trim_end_matches(['\r', '\n']).into(),
    })
}

pub fn collect(
    store: &mut Archive,
    container: &str,
    connection: Option<&str>,
    now: i64,
    earliest: i64,
) -> Result<usize> {
    migrate_legacy(store, container)?;
    let checkpoint = format!("podman_until:{container}");
    let since = store
        .get_state(&checkpoint)?
        .and_then(|s| s.parse::<i64>().ok())
        .map(|v| v.saturating_sub(1_000_000_000))
        .unwrap_or(earliest)
        .max(earliest);
    let mut cmd = command(connection);
    cmd.args([
        "logs",
        "--timestamps",
        "--since",
        &timestamps::format_ns(since),
        "--until",
        &timestamps::format_ns(now),
        container,
    ]);
    store.begin_podman_snapshot()?;
    let mut batch = Vec::new();
    let mut batch_bytes = 0;
    let mut added = 0;
    let mut invalid = 0;
    let mut diagnostic = String::new();
    let status = process::stream(&mut cmd, Duration::from_secs(120), |stderr, bytes| {
        let original = String::from_utf8_lossy(&bytes)
            .trim()
            .chars()
            .take(2048)
            .collect::<String>();
        match decode(bytes, stderr, store, now) {
            Ok(mut entry) => {
                entry.key = format!("{container}:{}", entry.key);
                if entry.event_ns >= earliest {
                    batch_bytes += entry.raw.len();
                    batch.push(entry);
                    if batch.len() >= BATCH_SIZE || batch_bytes >= 2 * 1024 * 1024 {
                        added += store.save_entries(&batch)?;
                        batch.clear();
                        batch_bytes = 0;
                    }
                }
            }
            Err(error) => {
                // Podman diagnostics use stderr too, alongside actual timestamped
                // container stderr. Preserve the original diagnostic, not only a parse error.
                invalid += 1;
                if diagnostic.len() < 8192 {
                    diagnostic.push_str(&format!("{original} ({error:#})\n"));
                }
            }
        }
        Ok(())
    })?;
    added += store.save_entries(&batch)?;
    if !status.success() || invalid != 0 {
        bail!(
            concat!(
                "Lettura Podman non completata (uscita: {status}; righe non riconosciute: ",
                "{invalid}). {diagnostic}I log già salvati restano disponibili; il punto di ",
                "lettura non è avanzato"
            ),
            status = status,
            invalid = invalid,
            diagnostic = diagnostic
        );
    }
    // Advance only after a complete successful snapshot, not after an out-of-order pipe batch.
    store.set_state(&checkpoint, &now.to_string())?;
    Ok(added)
}

pub fn migrate_legacy(store: &Archive, container: &str) -> Result<()> {
    if store.get_state("podman_scoped_keys")?.is_none() {
        store.db.execute("UPDATE entries SET record_key=?1 || ':' || record_key WHERE record_key LIKE 'podman:%'",[container])?;
        if let Some(until) = store.get_state("podman_until")? {
            store.set_state(&format!("podman_until:{container}"), &until)?;
        }
        store.set_state("podman_scoped_keys", "1")?;
    }
    Ok(())
}
