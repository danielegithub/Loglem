//! Riconosce il formato di una riga di log dal contenuto, per qualsiasi programma:
//! JSON, common/combined, syslog (RFC 3164 e 5424), logfmt (`chiave=valore`) o testo libero.
//!
//! Da qui passano ricerca, filtri, statistiche e domande: indirizzo IP, codice di risposta,
//! percorso, durata e gravità si leggono con nomi di campo comuni, non con quelli di un
//! prodotto. Una riga che non si riconosce resta cercabile come testo.
use crate::query::Severity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Json,
    Combined,
    Syslog,
    Logfmt,
    Text,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Json => "JSON",
            Format::Combined => "common/combined",
            Format::Syslog => "syslog",
            Format::Logfmt => "logfmt",
            Format::Text => "testo libero",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Combined => "combined",
            Format::Syslog => "syslog",
            Format::Logfmt => "logfmt",
            Format::Text => "text",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "json" => Format::Json,
            "combined" => Format::Combined,
            "syslog" => Format::Syslog,
            "logfmt" => Format::Logfmt,
            "text" => Format::Text,
            _ => return None,
        })
    }
}

// Nomi di campo comuni a molti programmi (il punto indica un campo annidato).
const IP_KEYS: &[&str] = &[
    "remote_ip",
    "client_ip",
    "remote_addr",
    "ip",
    "client.ip",
    "clientip",
    "client_addr",
    "ClientHost",
    "ClientAddr",
    "request.remote_ip",
    "request.client_ip",
    "request.remote_addr",
    "http.client_ip",
    "req.remoteAddress",
    "remoteAddress",
    "source.ip",
];
const STATUS_KEYS: &[&str] = &[
    "status",
    "status_code",
    "statusCode",
    "response.status",
    "response_status",
    "DownstreamStatus",
    "res.statusCode",
    "http.status_code",
    "http.response.status_code",
];
const PATH_KEYS: &[&str] = &[
    "uri",
    "path",
    "url",
    "request.uri",
    "request_path",
    "RequestPath",
    "req.url",
    "request.path",
    "http.target",
    "http.url",
    "url.path",
];
const LEVEL_KEYS: &[&str] = &[
    "level",
    "severity",
    "lvl",
    "log.level",
    "levelname",
    "loglevel",
    "log_level",
    "PRIORITY",
];

/// La parte utile di una riga: il JSON scritto dopo un prefisso (`programma[pid]: {…}`).
fn payload(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with('{') {
        return trimmed;
    }
    match line.find(": {") {
        Some(i) if !line[..i].contains('{') => &line[i + 2..],
        _ => line,
    }
}

fn json(line: &str) -> Option<Value> {
    let text = payload(line).trim_end();
    if text.starts_with('{') && text.ends_with('}') {
        serde_json::from_str(text).ok()
    } else {
        None
    }
}

fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    value
        .get(path)
        .or_else(|| path.split('.').try_fold(value, |v, k| v.get(k)))
}

fn first_text<'a>(value: &'a Value, keys: &[&str]) -> Option<Cow<'a, str>> {
    keys.iter().find_map(|k| match lookup(value, k)? {
        Value::String(s) if !s.is_empty() => Some(Cow::Borrowed(s.as_str())),
        Value::Number(n) => Some(Cow::Owned(n.to_string())),
        _ => None,
    })
}

fn first_status(value: &Value) -> Option<u16> {
    STATUS_KEYS
        .iter()
        .find_map(|k| match lookup(value, k)? {
            Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
            Value::String(s) => s.parse().ok(),
            _ => None,
        })
        .filter(|s| (100..600).contains(s))
}

/// `chiave=valore` e `chiave="valore con spazi"`.
fn logfmt(line: &str) -> Vec<(&str, Cow<'_, str>)> {
    let mut pairs = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'=' && bytes[i] != b' ' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' || i == start {
            while i < bytes.len() && bytes[i] != b' ' {
                i += 1;
            }
            continue;
        }
        let key = &line[start..i];
        i += 1;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let begin = i;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            let end = i.min(bytes.len());
            pairs.push((key, Cow::Owned(line[begin..end].replace("\\\"", "\""))));
            i += 1;
        } else {
            let begin = i;
            while i < bytes.len() && bytes[i] != b' ' {
                i += 1;
            }
            pairs.push((key, Cow::Borrowed(&line[begin..i])));
        }
    }
    pairs
}

fn logfmt_value<'a>(pairs: &'a [(&str, Cow<'a, str>)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| {
        pairs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_ref())
    })
}

fn is_logfmt(line: &str) -> bool {
    let pairs = logfmt(line);
    pairs.len() >= 2
        && pairs
            .iter()
            .any(|(k, _)| ["level", "lvl", "msg", "message", "time", "ts", "status"].contains(k))
}

/// Una richiesta HTTP in formato common/combined (anche con l'host virtuale davanti).
struct Combined<'a> {
    ip: Option<&'a str>,
    path: &'a str,
    status: u16,
}

fn combined(line: &str) -> Option<Combined<'_>> {
    let bracket = line.find(" [")?;
    let quote = line[bracket..].find("] \"")? + bracket + 3;
    let end = quote + line[quote..].find('"')?;
    let mut request = line[quote..end].split_whitespace();
    let method = request.next()?;
    if !method.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let path = request.next()?;
    let status: u16 = line[end + 1..].split_whitespace().next()?.parse().ok()?;
    if !(100..600).contains(&status) {
        return None;
    }
    let ip = line[..bracket]
        .split_whitespace()
        .find(|w| w.parse::<std::net::IpAddr>().is_ok());
    Some(Combined { ip, path, status })
}

/// `<PRI>` all'inizio della riga (syslog): la gravità è PRI modulo 8.
fn syslog_priority(line: &str) -> Option<u8> {
    let rest = line.trim_start().strip_prefix('<')?;
    let end = rest.find('>')?;
    if end == 0 || end > 3 {
        return None;
    }
    rest[..end].parse::<u16>().ok().map(|n| (n % 8) as u8)
}

fn is_syslog(line: &str) -> bool {
    if syslog_priority(line).is_some() {
        return true;
    }
    // RFC 3164: «Nov 18 10:00:00 host programma[pid]: …»
    let mut words = line.split_whitespace();
    let month = words.next().unwrap_or("");
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .contains(&month)
        && words
            .next()
            .and_then(|d| d.parse::<u8>().ok())
            .is_some_and(|d| (1..=31).contains(&d))
        && words
            .next()
            .is_some_and(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M:%S").is_ok())
}

/// Il formato di una riga.
pub fn of_line(line: &str) -> Format {
    if json(line).is_some() {
        Format::Json
    } else if combined(line).is_some() {
        Format::Combined
    } else if is_syslog(line) {
        Format::Syslog
    } else if is_logfmt(line) {
        Format::Logfmt
    } else {
        Format::Text
    }
}

/// Il formato di un log, da un campione di righe: quello più frequente, se riguarda almeno
/// metà delle righe; altrimenti testo libero.
pub fn detect<'a>(lines: impl IntoIterator<Item = &'a str>) -> Format {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0;
    for line in lines.into_iter().filter(|l| !l.trim().is_empty()).take(200) {
        *counts.entry(of_line(line)).or_insert(0) += 1;
        total += 1;
    }
    counts
        .into_iter()
        .filter(|(f, n)| *f != Format::Text && n * 2 >= total)
        .max_by_key(|(_, n)| *n)
        .map_or(Format::Text, |(f, _)| f)
}

// ---------- gravità ----------

fn level_word(word: &str) -> Option<Severity> {
    match word.trim().to_ascii_lowercase().as_str() {
        "error" | "err" | "e" | "fatal" | "critical" | "crit" | "panic" | "alert" | "emerg"
        | "emergency" | "severe" => Some(Severity::Error),
        "warn" | "warning" | "w" => Some(Severity::Warning),
        "info" | "information" | "informational" | "notice" | "debug" | "trace" | "i" | "d"
        | "verbose" => Some(Severity::Info),
        _ => None,
    }
}

/// Numeri: da 0 a 7 come syslog (0–3 errore, 4 avviso); dal 10 in su come pino/bunyan
/// (50 e oltre errore, 40 avviso).
fn level_number(n: f64) -> Severity {
    if n >= 10.0 {
        if n >= 50.0 {
            Severity::Error
        } else if n >= 40.0 {
            Severity::Warning
        } else {
            Severity::Info
        }
    } else if n <= 3.0 {
        Severity::Error
    } else if n <= 4.0 {
        Severity::Warning
    } else {
        Severity::Info
    }
}

fn json_severity(value: &Value) -> Option<Severity> {
    LEVEL_KEYS
        .iter()
        .find_map(|k| match lookup(value, k)? {
            Value::String(s) => s
                .parse::<f64>()
                .ok()
                .map(level_number)
                .or_else(|| level_word(s)),
            Value::Number(n) => n.as_f64().map(level_number),
            _ => None,
        })
        .or_else(|| {
            // Un log di accesso senza gravità: le risposte 5xx sono errori, le 4xx avvisi.
            first_status(value).map(status_severity)
        })
}

fn status_severity(status: u16) -> Severity {
    match status {
        500..=599 => Severity::Error,
        _ => Severity::Info,
    }
}

/// glog e simili: `E1118 10:00:00.123 …`, `W1118 …`.
fn glog(line: &str) -> Option<Severity> {
    let bytes = line.as_bytes();
    if bytes.len() > 6 && bytes[1..5].iter().all(u8::is_ascii_digit) && bytes[5] == b' ' {
        match bytes[0] {
            b'E' | b'F' => return Some(Severity::Error),
            b'W' => return Some(Severity::Warning),
            b'I' => return Some(Severity::Info),
            _ => {}
        }
    }
    None
}

/// Parole in maiuscolo (`ERROR`, `WARN`…), tra parentesi quadre (`[error]`, `[core:warn]`)
/// o seguite dai due punti (`Error:`, `Warning:`).
fn words(line: &str) -> Severity {
    let mut found = Severity::Info;
    fn raise(found: &mut Severity, s: Severity) {
        if s as u8 > *found as u8 {
            *found = s;
        }
    }
    for raw in line.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let token = raw.trim_matches(|c: char| c == '(' || c == ')' || c == '"' || c == '\'');
        if let Some(inner) = token.strip_prefix('[').and_then(|t| t.split(']').next()) {
            // Nginx «[error]», Apache «[core:error]», «[WARNING]».
            if let Some(s) = level_word(inner.rsplit(':').next().unwrap_or(inner)) {
                if inner.len() > 1 {
                    raise(&mut found, s);
                }
            }
            continue;
        }
        let bare = token.trim_end_matches(':');
        let upper = bare.len() > 2 && bare.chars().all(|c| c.is_ascii_uppercase());
        if upper {
            match bare {
                "ERROR" | "ERR" | "FATAL" | "CRITICAL" | "CRIT" | "PANIC" | "ALERT" | "EMERG"
                | "EMERGENCY" | "SEVERE" => raise(&mut found, Severity::Error),
                "WARN" | "WARNING" => raise(&mut found, Severity::Warning),
                _ => {}
            }
        } else if token.ends_with(':') && (bare == "Error" || bare == "Fatal") {
            raise(&mut found, Severity::Error);
        } else if token.ends_with(':') && bare == "Warning" {
            raise(&mut found, Severity::Warning);
        }
        if found == Severity::Error {
            break;
        }
    }
    found
}

/// La gravità di una riga, uguale per ricerca, filtri, colori e cancellazioni.
pub fn severity(line: &str) -> Severity {
    if let Some(value) = json(line) {
        if let Some(s) = json_severity(&value) {
            return s;
        }
    }
    if let Some(p) = syslog_priority(line) {
        return level_number(f64::from(p));
    }
    let pairs = logfmt(line);
    if let Some(level) = logfmt_value(&pairs, &["level", "lvl", "severity"]) {
        if let Some(s) = level_word(level) {
            return s;
        }
    }
    if let Some(c) = combined(line) {
        return status_severity(c.status);
    }
    if let Some(s) = glog(line) {
        return s;
    }
    words(line)
}

// ---------- campi di una richiesta ----------

/// Il codice di risposta HTTP, se la riga lo contiene in un formato riconosciuto.
pub fn status(line: &str) -> Option<u16> {
    if let Some(value) = json(line) {
        return first_status(&value);
    }
    if let Some(c) = combined(line) {
        return Some(c.status);
    }
    let pairs = logfmt(line);
    logfmt_value(&pairs, &["status", "status_code", "code"])
        .and_then(|s| s.parse().ok())
        .filter(|s| (100..600).contains(s))
}

/// Una richiesta HTTP riconosciuta in una riga di log.
#[derive(Debug, PartialEq)]
pub(crate) struct Request {
    pub(crate) path: String,
    pub(crate) status: u16,
    pub(crate) ip: Option<String>,
    pub(crate) duration_ms: Option<f64>,
}

fn short_path(uri: &str) -> String {
    let path = uri.split('?').next().unwrap_or_default();
    let mut end = path.len().min(256);
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    path[..end].to_owned()
}

fn json_duration_ms(value: &Value) -> Option<f64> {
    // (campo, quanti millisecondi vale un'unità)
    const DURATIONS: &[(&str, f64)] = &[
        ("duration_ms", 1.0),
        ("response_time", 1.0),
        ("responseTime", 1.0),
        ("elapsed_ms", 1.0),
        ("Duration", 1e-6),
        ("duration", 1000.0),
        ("request_time", 1000.0),
    ];
    DURATIONS
        .iter()
        .find_map(|(k, factor)| lookup(value, k)?.as_f64().map(|v| v * factor))
}

/// Una richiesta HTTP (percorso e codice almeno), da qualsiasi formato riconosciuto.
pub(crate) fn request(line: &str) -> Option<Request> {
    if let Some(value) = json(line) {
        let status = first_status(&value)?;
        let path = first_text(&value, PATH_KEYS)?;
        return Some(Request {
            path: short_path(&path),
            status,
            ip: first_text(&value, IP_KEYS).map(Cow::into_owned),
            duration_ms: json_duration_ms(&value),
        });
    }
    if let Some(c) = combined(line) {
        return Some(Request {
            path: short_path(c.path),
            status: c.status,
            ip: c.ip.map(str::to_owned),
            duration_ms: None,
        });
    }
    let pairs = logfmt(line);
    let status = logfmt_value(&pairs, &["status", "status_code"])?
        .parse()
        .ok()
        .filter(|s| (100..600).contains(s))?;
    let path = logfmt_value(&pairs, &["path", "uri", "url", "request_path"])?;
    Some(Request {
        path: short_path(path),
        status,
        ip: logfmt_value(&pairs, &["ip", "client_ip", "remote_ip", "remote_addr"])
            .map(str::to_owned),
        duration_ms: logfmt_value(&pairs, &["duration_ms", "response_time"])
            .and_then(|v| v.trim_end_matches("ms").parse().ok()),
    })
}

// ---------- formato salvato nella sorgente ----------

const FORMAT_EVERY_NS: i64 = 10 * 60_000_000_000;

/// Ricalcola il formato del log dalle righe più recenti (al massimo ogni 10 minuti) e lo
/// salva nell'archivio della sorgente: se il programma cambia formato, si aggiorna da solo.
pub fn refresh(archive: &crate::archive::Archive, now: i64) -> anyhow::Result<()> {
    let recent = archive
        .get_state("format_at")?
        .and_then(|v| v.parse::<i64>().ok())
        .is_some_and(|at| (0..FORMAT_EVERY_NS).contains(&(now - at)));
    if recent {
        return Ok(());
    }
    let mut stmt = archive
        .db
        .prepare("SELECT message FROM log_rows ORDER BY id DESC LIMIT 100")?;
    let lines: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if lines.is_empty() {
        return Ok(());
    }
    archive.set_state("format", detect(lines.iter().map(String::as_str)).name())?;
    archive.set_state("format_at", &now.to_string())
}

/// Il formato salvato per una sorgente, se è già stato riconosciuto.
pub fn saved(archive: &crate::archive::Archive) -> Option<Format> {
    archive
        .get_state("format")
        .ok()
        .flatten()
        .and_then(|n| Format::parse(&n))
}

// ---------- funzioni per SQLite ----------

/// Rende `loglem_severity(messaggio)` (0 informazione, 1 avviso, 2 errore) e
/// `loglem_status(messaggio)` usabili nelle query: così ricerca nell'archivio e
/// confronto sui file originali usano esattamente le stesse regole.
pub fn register(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    let flags = FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_UTF8;
    db.create_scalar_function("loglem_severity", 1, flags, |ctx| {
        let text = ctx.get_raw(0).as_str().unwrap_or("");
        Ok(severity(text) as i64)
    })?;
    db.create_scalar_function("loglem_status", 1, flags, |ctx| {
        let text = ctx.get_raw(0).as_str().unwrap_or("");
        Ok(status(text).map(i64::from))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_formats_from_content() {
        assert_eq!(of_line(r#"{"level":"info","msg":"ok"}"#), Format::Json);
        assert_eq!(
            of_line(r#"app[12]: {"level":"info","msg":"ok"}"#),
            Format::Json
        );
        assert_eq!(
            of_line(
                r#"203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] "GET /a HTTP/1.1" 200 5 "-" "x""#
            ),
            Format::Combined
        );
        assert_eq!(
            of_line(
                r#"shop.example:443 203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] "GET /a HTTP/1.1" 404 5"#
            ),
            Format::Combined
        );
        assert_eq!(
            of_line("Nov 18 10:00:00 host sshd[1]: Accepted"),
            Format::Syslog
        );
        assert_eq!(
            of_line("<165>1 2026-11-18T10:00:00Z host app - - - msg"),
            Format::Syslog
        );
        assert_eq!(
            of_line(r#"time=2026-11-18 level=warn msg="disk almost full""#),
            Format::Logfmt
        );
        assert_eq!(of_line("just some text"), Format::Text);
        assert_eq!(
            detect(["level=info msg=a", "level=info msg=b", "plain"]),
            Format::Logfmt
        );
        assert_eq!(detect(["a", "b"]), Format::Text);
    }

    #[test]
    fn severity_works_for_many_programs() {
        let cases = [
            (r#"{"level":"error","msg":"x"}"#, Severity::Error),
            (r#"{"severity":"WARNING","message":"x"}"#, Severity::Warning),
            (r#"{"level":50,"msg":"pino error"}"#, Severity::Error),
            (r#"{"level":40,"msg":"pino warn"}"#, Severity::Warning),
            (r#"{"level":30,"msg":"pino info"}"#, Severity::Info),
            (r#"{"log":{"level":"warn"}}"#, Severity::Warning),
            (r#"{"PRIORITY":"3","MESSAGE":"x"}"#, Severity::Error),
            (r#"{"status":503,"uri":"/a"}"#, Severity::Error),
            ("<11>Nov 18 10:00:00 host app: x", Severity::Error),
            ("<12>Nov 18 10:00:00 host app: x", Severity::Warning),
            ("<14>Nov 18 10:00:00 host app: x", Severity::Info),
            ("time=1 level=error msg=boom", Severity::Error),
            ("E1118 10:00:00.123456 1 main.go:12] boom", Severity::Error),
            (
                "W1118 10:00:00.123456 1 main.go:12] careful",
                Severity::Warning,
            ),
            (
                "2026/11/18 10:00:00 [error] 12#0: *1 open() failed",
                Severity::Error,
            ),
            (
                "[Wed Nov 18 10:00:00 2026] [core:warn] [pid 1] x",
                Severity::Warning,
            ),
            ("2026-11-18 10:00:00 CRIT disk failure", Severity::Error),
            ("PHP Warning: Undefined variable", Severity::Warning),
            ("Error: connection refused", Severity::Error),
            ("NO_ERRORS found in the report", Severity::Info),
            ("everything is fine", Severity::Info),
            (
                r#"203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] "GET /a HTTP/1.1" 500 5"#,
                Severity::Error,
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(severity(line), expected, "{line}");
        }
    }

    #[test]
    fn requests_from_json_combined_and_logfmt_of_any_program() {
        let json = request(r#"{"request":{"uri":"/a?x=1","remote_ip":"203.0.113.7"},"status":200,"duration":0.012}"#).unwrap();
        assert_eq!(
            (json.path.as_str(), json.status, json.ip.as_deref()),
            ("/a", 200, Some("203.0.113.7"))
        );
        assert!((json.duration_ms.unwrap() - 12.0).abs() < 1e-9);
        let other =
            request(r#"{"remote_addr":"192.0.2.1","request_path":"/b","status_code":"404"}"#)
                .unwrap();
        assert_eq!(
            (other.path.as_str(), other.status, other.ip.as_deref()),
            ("/b", 404, Some("192.0.2.1"))
        );
        let node = request(r#"{"req":{"url":"/c","remoteAddress":"192.0.2.2"},"res":{"statusCode":201},"responseTime":4}"#).unwrap();
        assert_eq!(
            (
                node.path.as_str(),
                node.status,
                node.ip.as_deref(),
                node.duration_ms
            ),
            ("/c", 201, Some("192.0.2.2"), Some(4.0))
        );
        let traefik = request(r#"{"RequestPath":"/d","ClientHost":"192.0.2.3","DownstreamStatus":502,"Duration":2000000}"#).unwrap();
        assert_eq!((traefik.status, traefik.duration_ms), (502, Some(2.0)));
        let combined =
            request(r#"203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] "POST /login HTTP/1.1" 302 5"#)
                .unwrap();
        assert_eq!(
            (
                combined.path.as_str(),
                combined.status,
                combined.ip.as_deref()
            ),
            ("/login", 302, Some("203.0.113.4"))
        );
        let logfmt =
            request(r#"level=info ip=192.0.2.9 path="/e" status=500 duration_ms=12"#).unwrap();
        assert_eq!(
            (logfmt.path.as_str(), logfmt.status, logfmt.ip.as_deref()),
            ("/e", 500, Some("192.0.2.9"))
        );
        assert!(request("plain text 404").is_none());
        assert_eq!(status(r#"{"status": 404}"#), Some(404));
        assert_eq!(status("level=info status=301 path=/x"), Some(301));
        assert_eq!(status("the code was 404"), None);
    }

    #[test]
    fn sql_functions_match_rust() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        register(&db).unwrap();
        let line = r#"{"level":"warn","status":404}"#;
        let (s, code): (i64, Option<i64>) = db
            .query_row(
                "SELECT loglem_severity(?1), loglem_status(?1)",
                [line],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(s, severity(line) as i64);
        assert_eq!(code, Some(404));
    }
}
