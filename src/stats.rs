//! Statistiche sui log di accesso, calcolate dall'archivio.
//!
//! Riconosce il JSON di Caddy e il formato «common/combined» di Nginx e Apache.
//! Le righe di altri formati vengono contate a parte, senza indovinare.
use crate::{archive::Archive, config};
use anyhow::{bail, Result};
use serde::Serialize;
use std::{collections::HashMap, path::Path};

/// Una richiesta HTTP riconosciuta in una riga di log.
#[derive(Debug, PartialEq)]
pub(crate) struct Request {
    path: String,
    pub(crate) status: u16,
    pub(crate) ip: Option<String>,
    duration_ms: Option<f64>,
}

pub(crate) fn parse(line: &str) -> Option<Request> {
    let payload = line
        .split_once(": {")
        .filter(|(prefix, _)| !prefix.contains('{'))
        .map_or(line, |(prefix, _)| &line[prefix.len() + 2..]);
    if payload.trim_start().starts_with('{') {
        parse_caddy(payload)
    } else {
        parse_combined(line)
    }
}

/// `{"request":{"uri":"/a?x=1","remote_ip":"…"},"status":200,"duration":0.012}`
fn parse_caddy(line: &str) -> Option<Request> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if let Some(path) = value["RequestPath"].as_str() {
        return Some(Request {
            path: path.split('?').next()?.into(),
            status: u16::try_from(value["DownstreamStatus"].as_u64()?).ok()?,
            ip: value["ClientHost"].as_str().map(str::to_owned),
            duration_ms: value["Duration"].as_f64().map(|v| v / 1_000_000.0),
        });
    }
    let request = value.get("request")?;
    let uri = request.get("uri")?.as_str()?;
    let ip = ["client_ip", "remote_ip"]
        .iter()
        .find_map(|key| request.get(key)?.as_str())
        .map(str::to_owned);
    Some(Request {
        path: uri.split('?').next().unwrap_or(uri).to_owned(),
        status: u16::try_from(value.get("status")?.as_u64()?).ok()?,
        ip,
        duration_ms: value
            .get("duration")
            .and_then(|d| d.as_f64())
            .map(|s| s * 1000.0),
    })
}

/// `203.0.113.7 - - [16/Nov/2026:10:00:00 +0000] "GET /index.html HTTP/1.1" 200 512 …`
fn parse_combined(line: &str) -> Option<Request> {
    let ip = line.split_whitespace().next()?.to_owned();
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    let mut request = line[start..end].split_whitespace();
    let _method = request.next()?;
    let target = request.next()?;
    let status = line[end + 1..].split_whitespace().next()?.parse().ok()?;
    Some(Request {
        path: target.split('?').next().unwrap_or(target).to_owned(),
        status,
        ip: ip.parse::<std::net::IpAddr>().is_ok().then_some(ip),
        duration_ms: None,
    })
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub source: String,
    pub days: u32,
    pub requests: u64,
    /// Righe che non sembrano richieste HTTP (formato sconosciuto).
    pub other_lines: u64,
    pub per_day: Vec<(String, u64)>,
    pub top_paths: Vec<(String, u64)>,
    pub statuses: Vec<(u16, u64)>,
    pub top_ips: Vec<(String, u64)>,
    /// Percorso, durata media in millisecondi, numero di richieste (solo se il log ha la durata).
    pub slowest: Vec<(String, f64, u64)>,
}

fn top<K: Clone + Ord>(counts: HashMap<K, u64>, n: usize) -> Vec<(K, u64)> {
    let mut list: Vec<_> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list.truncate(n);
    list
}

pub fn compute(dir: &Path, source: &str, days: u32) -> Result<Stats> {
    let config = config::load(dir)?;
    if !config.sources.iter().any(|s| s.id == source) {
        bail!("Sorgente non trovata");
    }
    let path = config::archive_path(dir, source);
    let since = crate::timestamps::now_ns() - i64::from(days) * 86_400 * 1_000_000_000;
    let mut stats = Stats {
        source: source.into(),
        days,
        requests: 0,
        other_lines: 0,
        per_day: Vec::new(),
        top_paths: Vec::new(),
        statuses: Vec::new(),
        top_ips: Vec::new(),
        slowest: Vec::new(),
    };
    if !path.exists() {
        return Ok(stats);
    }
    let (mut per_day, mut paths, mut statuses, mut ips) = (
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    );
    let mut durations: HashMap<String, (f64, u64)> = HashMap::new();
    let archive = Archive::open(&path)?;
    let mut select = archive
        .db
        .prepare("SELECT event_ns, message FROM log_rows WHERE event_ns >= ?1")?;
    let mut rows = select.query([since])?;
    while let Some(row) = rows.next()? {
        let message: String = row.get(1)?;
        let Some(request) = parse(&message) else {
            stats.other_lines += 1;
            continue;
        };
        stats.requests += 1;
        let day = crate::timestamps::format_ns(row.get(0)?)[..10].to_owned();
        *per_day.entry(day).or_insert(0) += 1;
        *statuses.entry(request.status).or_insert(0) += 1;
        if let Some(ip) = request.ip {
            *ips.entry(ip).or_insert(0) += 1;
        }
        if let Some(ms) = request.duration_ms {
            let entry = durations.entry(request.path.clone()).or_insert((0.0, 0));
            entry.0 += ms;
            entry.1 += 1;
        }
        *paths.entry(request.path).or_insert(0) += 1;
    }
    let mut per_day: Vec<_> = per_day.into_iter().collect();
    per_day.sort();
    stats.per_day = per_day;
    stats.top_paths = top(paths, 10);
    let mut statuses: Vec<_> = statuses.into_iter().collect();
    statuses.sort();
    stats.statuses = statuses;
    stats.top_ips = top(ips, 10);
    let mut slowest: Vec<_> = durations
        .into_iter()
        .map(|(path, (total, count))| (path, total / count as f64, count))
        .collect();
    slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
    slowest.truncate(10);
    stats.slowest = slowest;
    Ok(stats)
}

/// Le stesse statistiche in testo, per il terminale.
pub fn text(stats: &Stats) -> String {
    let mut out = format!(
        "{} · ultimi {} giorni · {} richieste ({} righe di altro formato)\n",
        stats.source, stats.days, stats.requests, stats.other_lines
    );
    let section = |out: &mut String, title: &str, rows: Vec<String>| {
        out.push_str(&format!("\n{title}\n"));
        for row in rows {
            out.push_str(&format!("  {row}\n"));
        }
    };
    section(
        &mut out,
        "Richieste per giorno",
        stats
            .per_day
            .iter()
            .map(|(d, n)| format!("{d}  {n}"))
            .collect(),
    );
    section(
        &mut out,
        "Pagine più richieste",
        stats
            .top_paths
            .iter()
            .map(|(p, n)| format!("{n:>7}  {p}"))
            .collect(),
    );
    section(
        &mut out,
        "Codici di risposta",
        stats
            .statuses
            .iter()
            .map(|(s, n)| format!("{s}  {n}"))
            .collect(),
    );
    section(
        &mut out,
        "Indirizzi più frequenti",
        stats
            .top_ips
            .iter()
            .map(|(ip, n)| format!("{n:>7}  {ip}"))
            .collect(),
    );
    if !stats.slowest.is_empty() {
        section(
            &mut out,
            "Pagine più lente (media)",
            stats
                .slowest
                .iter()
                .map(|(p, ms, n)| format!("{ms:>8.2} ms  {p} ({n} richieste)"))
                .collect(),
        );
    }
    out
}
