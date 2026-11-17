//! Domande trasversali agli archivi di un sito, con gli stessi filtri della ricerca.
use crate::{
    archive::Archive,
    config,
    model::SearchRow,
    query::{severity, Query},
    stats, timestamps, util,
};
use anyhow::{ensure, Result};
use rusqlite::types::Value as SqlValue;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub fn search(dir: &Path, body: &Value, from: i64, until: i64) -> Result<Value> {
    let _access = util::DataAccess::read(dir)?;
    let cfg = config::load(dir)?;
    let query: Query = serde_json::from_value(body.clone())?;
    let question = body["question"].as_str().unwrap_or("");
    ensure!(
        matches!(question, "" | "admin" | "cron" | "5xx" | "404"),
        "Domanda sconosciuta"
    );
    let site = body["site"].as_str().unwrap_or("");
    let source = body["source"].as_str().unwrap_or("");
    ensure!(from < until, "Intervallo non valido");
    let mut found = Vec::new();
    let mut counts = BTreeMap::new();
    for s in cfg
        .sources
        .iter()
        .filter(|s| (site.is_empty() || s.site == site) && (source.is_empty() || s.id == source))
    {
        let path = s.archive_path(dir);
        if !path.is_file() {
            continue;
        }
        let db = Archive::open(&path)?;
        if question.is_empty() {
            found.extend(db.search(&s.id, &query, from, until, 200)?);
        } else {
            let (filter, mut args) = query.sql();
            args.insert(0, SqlValue::Integer(until));
            args.insert(0, SqlValue::Integer(from));
            let mut stmt=db.db.prepare(&format!("SELECT event_ns,timestamp_origin,message FROM log_rows WHERE event_ns>=? AND event_ns<? {filter} ORDER BY event_ns DESC,id DESC"))?;
            let mut rows = stmt.query(rusqlite::params_from_iter(args))?;
            let mut n = 0;
            while let Some(row) = rows.next()? {
                let message: String = row.get(2)?;
                let matches = match question {
                    "admin" => admin_login(&message),
                    "cron" => {
                        let l = message.to_lowercase();
                        l.contains("cron")
                            && [
                                "fail",
                                "error",
                                "bad exit",
                                "non-zero",
                                "nonzero",
                                "exit status 1",
                            ]
                            .iter()
                            .any(|word| l.contains(word))
                    }
                    "5xx" => stats::parse(&message).is_some_and(|r| (500..600).contains(&r.status)),
                    "404" => {
                        if let Some(ip) = stats::parse(&message)
                            .filter(|r| r.status == 404)
                            .and_then(|r| r.ip)
                        {
                            *counts.entry(ip).or_insert(0u64) += 1;
                        }
                        false
                    }
                    _ => false,
                };
                if matches {
                    found.push(SearchRow {
                        source: s.id.clone(),
                        timestamp: timestamps::format_ns(row.get(0)?),
                        timestamp_origin: row.get(1)?,
                        level: severity(&message).name().into(),
                        message,
                    });
                    n += 1;
                    if n == 200 {
                        break;
                    }
                }
            }
        }
        found.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        found.truncate(200);
    }
    let mut ranking: Vec<_> = counts.into_iter().collect();
    ranking.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranking.truncate(200);
    Ok(json!({"rows":found,"ranking":ranking}))
}
fn admin_login(message: &str) -> bool {
    let line = message.to_ascii_lowercase();
    let user = if line.contains("accepted ") {
        line.split_once(" for ").map(|v| v.1)
    } else {
        line.split_once("session opened for user ").map(|v| v.1)
    };
    user.and_then(|v| v.split(|c: char| c.is_whitespace() || c == '(').next())
        .is_some_and(|u| matches!(u, "root" | "deploy"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admin_logins_exclude_failures_and_similar_names() {
        assert!(admin_login(
            "sshd[7]: Accepted publickey for deploy from 192.0.2.1"
        ));
        assert!(admin_login(
            "pam_unix(sudo:session): session opened for user root(uid=0)"
        ));
        assert!(!admin_login("sshd: Failed password for root"));
        assert!(!admin_login("sshd: Accepted password for deploybot"));
    }
    #[test]
    fn traefik_downstream_status_and_event_time() {
        let line = r#"{"RequestPath":"/users?q=a","StartUTC":"2025-01-02T03:04:05Z","ClientHost":"192.0.2.1","OriginStatus":200,"DownstreamStatus":503,"Duration":2000000}"#;
        let parsed = stats::parse(line).unwrap();
        assert_eq!(parsed.status, 503);
        assert_eq!(parsed.ip.as_deref(), Some("192.0.2.1"));
        assert_eq!(
            timestamps::event_time(line, 0),
            (
                timestamps::parse_date("2025-01-02T03:04:05Z", false).unwrap(),
                "event"
            )
        );
    }
}
