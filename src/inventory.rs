//! Mappa in sola lettura delle sorgenti visibili all'utente del servizio.
use crate::{
    config,
    model::Input,
    process,
    sources::{journal, podman},
};
use anyhow::Result;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn scan(dir: &Path) -> Result<Value> {
    let cfg = config::load(dir)?;
    let mut items = BTreeMap::new();
    let mut notes = Vec::new();
    let mut dirs = vec![PathBuf::from("/var/log"), PathBuf::from("/srv")];
    let mut visited = 0;
    'walk: while let Some(folder) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            if folder.exists() {
                notes.push(format!("Non leggibile: {}", folder.display()));
            }
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > 100_000 || items.len() >= 4000 {
                notes.push("Scansione parziale: raggiunto il limite di voci".into());
                break 'walk;
            }
            let path = entry.path();
            let Ok(ty) = entry.file_type() else { continue };
            if ty.is_dir() {
                dirs.push(path);
                continue;
            }
            if ty.is_file() && crate::sources::file::log_name(&path) {
                let bytes = entry.metadata().ok().map(|v| v.len());
                items.insert(
                    path.display().to_string(),
                    json!({"kind":"file","path":path,"bytes":bytes,"program":"File"}),
                );
            }
        }
    }
    for f in crate::discover::find_all() {
        items.insert(f.path.display().to_string(),json!({"kind":f.kind,"path":f.path,"bytes":fs::metadata(&f.path).ok().map(|m|m.len()),"program":f.program}));
    }
    for rt in [podman::Runtime::Podman, podman::Runtime::Docker] {
        match podman::discover_with(rt, None) {
            Ok(list) => {
                for container in list {
                    let size = process::output(
                        rt.command(None).args([
                            "inspect",
                            "--format",
                            "{{.LogPath}}",
                            "--",
                            &container.id,
                        ]),
                        Duration::from_secs(3),
                    )
                    .ok()
                    .and_then(|out| fs::metadata(String::from_utf8_lossy(&out).trim()).ok())
                    .map(|m| m.len());
                    items.insert(format!("{}:{}",rt.name(),container.name),json!({"kind":rt,"container":container.id,"name":container.name,"bytes":size,"program":rt.name()}));
                }
            }
            Err(_) => notes.push(format!("{} non disponibile", rt.name())),
        }
    }
    match journal::units() {
        Ok(units) => {
            for unit in units {
                items.insert(format!("journal:{unit}"),json!({"kind":"journal","journal":{"units":[unit]},"name":unit,"program":"Journal"}));
            }
        }
        Err(_) => notes.push("Journal non disponibile".into()),
    }
    items.insert("preset:cron".into(),json!({"kind":"journal","journal":{"identifiers":["CRON","CROND","cron","crond"]},"name":"cron","program":"Cron"}));
    for item in items.values_mut() {
        item["connected"] =
            json!(cfg
                .sources
                .iter()
                .filter(|s| match &s.input {
                    Input::File {
                        path,
                        rotated_pattern,
                    } => item["path"].as_str().is_some_and(|p| {
                        let p = Path::new(p);
                        p == path
                            || (p.parent() == path.parent()
                                && rotated_pattern.as_ref().is_some_and(|pat| {
                                    glob::Pattern::new(pat).ok().is_some_and(|g| {
                                        p.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
                                            g.matches(v.strip_suffix(".gz").unwrap_or(v))
                                        })
                                    })
                                }))
                    }),
                    Input::Folder { path } => item["path"]
                        .as_str()
                        .is_some_and(|p| Path::new(p).parent() == Some(path)
                            && (p.ends_with(".log") || p.ends_with(".log.gz"))),
                    Input::Journal(f) =>
                        if item["kind"] == "journal" {
                            if item["program"] == "Cron" {
                                f.identifiers.iter().any(|i| {
                                    i.eq_ignore_ascii_case("cron")
                                        || i.eq_ignore_ascii_case("crond")
                                })
                            } else {
                                f == &journal::Filter::default()
                                    || f.units.iter().any(|u| item["name"] == *u)
                            }
                        } else {
                            matches!(item["kind"].as_str(), Some("podman" | "docker"))
                                && f.matches.contains(&format!(
                                    "CONTAINER_NAME={}",
                                    item["name"].as_str().unwrap_or("")
                                ))
                        },
                    Input::Podman {
                        container,
                        connection,
                    } =>
                        connection.is_none()
                            && item["kind"] == "podman"
                            && item["container"] == *container,
                    Input::Docker { container, context } =>
                        context.is_none()
                            && item["kind"] == "docker"
                            && item["container"] == *container,
                    _ => false,
                })
                .map(|s| &s.id)
                .collect::<Vec<_>>());
    }
    Ok(json!({"items":items.into_values().collect::<Vec<_>>(),"notes":notes}))
}

/// Il journal condivide e comprime i file tra servizi. Misuriamo il contenuto leggibile
/// separatamente dall'occupazione fisica, che non è attribuibile a una singola unità.
pub fn message_bytes(item: &Value) -> Result<Value> {
    use anyhow::{ensure, Context};
    use std::process::Command;
    let mut bytes = 0u64;
    let journal = item["kind"] == "journal";
    let mut cmd = if journal {
        let filter: journal::Filter = serde_json::from_value(item["journal"].clone())?;
        filter.validate()?;
        let mut cmd = Command::new("journalctl");
        cmd.args([
            "--quiet",
            "--no-pager",
            "--output=json",
            "--output-fields=MESSAGE",
        ]);
        filter.apply(&mut cmd);
        cmd
    } else {
        let rt: podman::Runtime = serde_json::from_value(item["kind"].clone())?;
        let id = item["container"].as_str().context("Container mancante")?;
        ensure!(
            id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "ID container non valido"
        );
        let mut cmd = rt.command(None);
        cmd.args(["logs", "--", id]);
        cmd
    };
    let status = process::stream(&mut cmd, Duration::from_secs(45), |stderr, line| {
        if journal {
            if !stderr {
                let v: Value = serde_json::from_slice(&line)?;
                bytes += match &v["MESSAGE"] {
                    Value::String(s) => s.len(),
                    Value::Array(b) => b.len(),
                    _ => 0,
                } as u64;
            }
        } else {
            bytes += line.len() as u64;
        }
        Ok(())
    })?;
    ensure!(status.success(), "Misurazione non disponibile");
    Ok(json!({"bytes":bytes}))
}
