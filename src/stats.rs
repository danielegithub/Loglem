//! Statistiche sui log di accesso, calcolate dall'archivio.
//!
//! Riconosce le richieste HTTP in JSON (nomi di campo comuni), common/combined e logfmt,
//! per qualsiasi programma. Le righe di altri formati vengono contate a parte.
use crate::{archive::Archive, config};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::{collections::HashMap, path::Path};

pub(crate) use crate::formats::Request;

/// Una richiesta HTTP in qualsiasi formato riconosciuto (JSON, combined, logfmt).
pub(crate) fn parse(line: &str) -> Option<Request> {
    crate::formats::request(line)
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub approximate: bool,
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

const DAY: i64 = 86_400_000_000_000;
const CAPACITY: usize = 256;

#[derive(Default, Serialize, Deserialize, Clone, Copy)]
struct Count {
    n: u64,
    ms: f64,
    timed: u64,
}

fn add<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Count>, key: K, mut v: Count) -> bool {
    let mut approximate = false;
    if !map.contains_key(&key) && map.len() >= CAPACITY {
        approximate = true;
        let decrement = map.values().map(|x| x.n).min().unwrap().min(v.n);
        map.retain(|_, x| {
            x.n -= decrement;
            x.n > 0
        });
        v.n -= decrement;
    }
    if v.n > 0 {
        let count = map.entry(key).or_default();
        count.n += v.n;
        count.ms += v.ms;
        count.timed += v.timed;
    }
    approximate
}

#[derive(Default, Serialize, Deserialize)]
struct Daily {
    requests: u64,
    other: u64,
    approximate: bool,
    paths: HashMap<String, Count>,
    ips: HashMap<IpAddr, Count>,
    statuses: HashMap<u16, u64>,
}
impl Daily {
    fn record(&mut self, line: &str) {
        let Some(req) = parse(line) else {
            self.other += 1;
            return;
        };
        self.requests += 1;
        *self.statuses.entry(req.status).or_default() += 1;
        let count = Count {
            n: 1,
            ..Default::default()
        };
        self.approximate |= add(
            &mut self.paths,
            req.path,
            Count {
                ms: req.duration_ms.unwrap_or_default(),
                timed: u64::from(req.duration_ms.is_some()),
                ..count
            },
        );
        if let Some(ip) = req.ip.and_then(|s| s.parse().ok()) {
            self.approximate |= add(&mut self.ips, ip, count);
        }
    }
    fn merge(&mut self, day: Daily) {
        self.requests += day.requests;
        self.other += day.other;
        self.approximate |= day.approximate;
        for (k, v) in day.paths {
            self.approximate |= add(&mut self.paths, k, v);
        }
        for (k, v) in day.ips {
            self.approximate |= add(&mut self.ips, k, v);
        }
        for (k, v) in day.statuses {
            *self.statuses.entry(k).or_default() += v;
        }
    }
}

fn read_day(
    db: &rusqlite::Connection,
    day: i64,
    since: i64,
    upto: i64,
    stats: &mut Daily,
) -> Result<i64> {
    let mut stmt =
        db.prepare("SELECT id,message FROM log_rows WHERE last_id>?3 AND last_ns>=?1 AND first_ns<?2 AND event_ns>=?1 AND event_ns<?2 AND id>?3")?;
    let mut rows = stmt.query(rusqlite::params![
        day.saturating_mul(DAY).max(since),
        (day + 1).saturating_mul(DAY),
        upto
    ])?;
    let mut last = upto;
    while let Some(row) = rows.next()? {
        last = last.max(row.get(0)?);
        stats.record(row.get_ref(1)?.as_str()?);
    }
    Ok(last)
}

pub(crate) fn refresh(archive: &Archive) -> Result<()> {
    loop {
        let tx = archive.db.unchecked_transaction()?;
        use rusqlite::OptionalExtension;
        let item = tx
            .query_row(
                "SELECT day,upto,summary FROM daily_stats WHERE dirty=1 LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((day, upto, summary)) = item else {
            return Ok(());
        };
        let mut counts: Daily = summary
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default();
        let last = read_day(&tx, day, i64::MIN, upto, &mut counts)?;
        if last == 0 {
            tx.execute("DELETE FROM daily_stats WHERE day=?", [day])?;
        } else {
            tx.execute(
                "UPDATE daily_stats SET upto=?,summary=?,dirty=0 WHERE day=?",
                rusqlite::params![last, serde_json::to_string(&counts)?, day],
            )?;
        }
        tx.commit()?;
    }
}

pub fn compute(dir: &Path, source: &str, days: u32) -> Result<Stats> {
    let source_cfg = config::source(dir, source)?;
    let path = source_cfg.archive_path(dir);
    let since = crate::timestamps::now_ns().saturating_sub(i64::from(days).saturating_mul(DAY));
    let mut all = Daily::default();
    let mut per_day = vec![];
    if path.exists() {
        let archive = Archive::open(&path)?;
        let tx = archive.db.unchecked_transaction()?;
        let mut stmt =
            tx.prepare("SELECT day,upto,summary,dirty FROM daily_stats WHERE day>=? ORDER BY day")?;
        let mut rows = stmt.query([since.div_euclid(DAY)])?;
        while let Some(row) = rows.next()? {
            let day: i64 = row.get(0)?;
            let partial = day.saturating_mul(DAY) < since;
            let mut counts: Daily = if partial {
                Daily::default()
            } else {
                row.get::<_, Option<String>>(2)?
                    .map(|v| serde_json::from_str(&v))
                    .transpose()?
                    .unwrap_or_default()
            };
            if partial || row.get::<_, bool>(3)? {
                read_day(
                    &tx,
                    day,
                    since,
                    if partial { 0 } else { row.get(1)? },
                    &mut counts,
                )?;
            }
            if counts.requests > 0 {
                per_day.push((
                    crate::timestamps::format_ns(day.saturating_mul(DAY))[..10].to_owned(),
                    counts.requests,
                ));
            }
            all.merge(counts);
        }
    }
    let mut slowest: Vec<_> = all
        .paths
        .iter()
        .filter(|(_, v)| v.timed > 0)
        .map(|(k, v)| (k.clone(), v.ms / v.timed as f64, v.timed))
        .collect();
    slowest.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    slowest.truncate(10);
    let mut statuses: Vec<_> = all.statuses.into_iter().collect();
    statuses.sort();
    Ok(Stats {
        source: source.into(),
        days,
        requests: all.requests,
        other_lines: all.other,
        approximate: all.approximate,
        per_day,
        statuses,
        slowest,
        top_ips: top(
            all.ips
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.n))
                .collect(),
            10,
        ),
        top_paths: top(all.paths.into_iter().map(|(k, v)| (k, v.n)).collect(), 10),
    })
}

/// Le stesse statistiche in testo, per il terminale.
pub fn text(stats: &Stats) -> String {
    let mut out = format!(
        "{} · ultimi {} giorni · {} richieste ({} righe di altro formato)\n",
        stats.source, stats.days, stats.requests, stats.other_lines
    );
    if stats.approximate {
        out.push_str("Classifica approssimata\n");
    }
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
