//! Cosa cercare nei log: testo e filtri facoltativi, tradotti in condizioni SQL.
//!
//! I log non hanno un formato fisso, quindi gravità, codice di risposta e percorso
//! si riconoscono dal testo della riga (JSON di Caddy, formato «combined» di Nginx e
//! Apache, journal). Sono filtri pratici, non un'analisi completa del formato.
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Errori e più gravi.
    Error,
    /// Avvisi, errori e più gravi.
    Warning,
}

/// Gravità di una riga, riconosciuta dal testo. Ogni riga ha esattamente una gravità:
/// quello che non è né errore né avviso è informazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// La gravità di una riga, con le stesse regole dei filtri.
pub fn severity(line: &str) -> Severity {
    if ERROR_MARKS.iter().any(|m| line.contains(m)) {
        Severity::Error
    } else if WARNING_MARKS.iter().any(|m| line.contains(m)) {
        Severity::Warning
    } else {
        Severity::Info
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, clap::Args)]
pub struct Query {
    /// Testo da cercare (vuoto = tutto).
    #[arg(long, default_value = "")]
    #[serde(default)]
    pub text: String,
    /// Non distinguere maiuscole e minuscole.
    #[arg(long)]
    #[serde(default)]
    pub ignore_case: bool,
    /// Solo righe di questa gravità o più gravi.
    #[arg(long, value_enum)]
    #[serde(default)]
    pub level: Option<Level>,
    /// Solo le righe di queste gravità (vuoto o tutte e tre = non filtra). Lo usa la pagina web.
    #[arg(skip)]
    #[serde(default)]
    pub only: Vec<Severity>,
    /// Solo righe che contengono questo indirizzo IP.
    #[arg(long)]
    #[serde(default)]
    pub ip: Option<String>,
    /// Solo richieste a questo percorso (es. /login).
    #[arg(long)]
    #[serde(default)]
    pub path: Option<String>,
    /// Solo risposte con questo codice (es. 404).
    #[arg(long)]
    #[serde(default)]
    pub status: Option<u16>,
}

/// Come compaiono gli errori nei formati più comuni.
const ERROR_MARKS: &[&str] = &[
    "\"level\":\"error\"",
    "\"level\":\"fatal\"",
    "\"level\":\"panic\"",
    "\"PRIORITY\":\"0\"",
    "\"PRIORITY\":\"1\"",
    "\"PRIORITY\":\"2\"",
    "\"PRIORITY\":\"3\"",
    "ERROR",
    "FATAL",
    "CRITICAL",
    "PANIC",
    "[error]",
    "[crit]",
    "[alert]",
    "[emerg]",
];
const WARNING_MARKS: &[&str] = &[
    "\"level\":\"warn",
    "\"PRIORITY\":\"4\"",
    "WARN",
    "[warn]",
    "Warning",
];

impl Query {
    /// Vero se la riga corrisponde ai filtri, con le stesse regole di `sql()`:
    /// serve a trovare le righe da togliere dai file originali.
    pub fn matches(&self, line: &str) -> bool {
        let has = |needle: &str| line.contains(needle);
        if !self.text.is_empty() {
            let found = if self.ignore_case {
                line.to_lowercase().contains(&self.text.to_lowercase())
            } else {
                has(&self.text)
            };
            if !found {
                return false;
            }
        }
        for needle in [&self.ip, &self.path].into_iter().flatten() {
            if !needle.is_empty() && !has(needle) {
                return false;
            }
        }
        if let Some(status) = self.status {
            if !has(&format!("\"status\":{status}"))
                && !has(&format!("\"DownstreamStatus\":{status}"))
                && !has(&format!("\" {status} "))
            {
                return false;
            }
        }
        if let Some(level) = self.level {
            let mut marks = ERROR_MARKS.to_vec();
            if level == Level::Warning {
                marks.extend(WARNING_MARKS);
            }
            if !marks.iter().any(|m| has(m)) {
                return false;
            }
        }
        !self.filters_severity() || self.only.contains(&severity(line))
    }

    /// Il filtro per gravità toglie qualcosa solo se è scelto un sottoinsieme.
    fn filters_severity(&self) -> bool {
        let mut chosen = self.only.clone();
        chosen.sort_by_key(|s| *s as u8);
        chosen.dedup();
        !chosen.is_empty() && chosen.len() < 3
    }

    pub fn text(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    /// Condizioni da aggiungere dopo `WHERE …` (ognuna inizia con `AND`), con
    /// segnaposto `?` e i parametri corrispondenti, nello stesso ordine.
    pub fn sql(&self) -> (String, Vec<Value>) {
        let mut sql = String::new();
        let mut params = Vec::new();
        if !self.text.is_empty() {
            if self.ignore_case {
                sql.push_str(" AND instr(lower(message), ?) > 0");
                params.push(Value::Text(self.text.to_lowercase()));
            } else {
                sql.push_str(" AND instr(message, ?) > 0");
                params.push(Value::Text(self.text.clone()));
            }
        }
        for needle in [&self.ip, &self.path].into_iter().flatten() {
            if !needle.is_empty() {
                sql.push_str(" AND instr(message, ?) > 0");
                params.push(Value::Text(needle.clone()));
            }
        }
        if let Some(status) = self.status {
            // JSON di Caddy: "status":404 · formato combined: "GET / HTTP/1.1" 404
            sql.push_str(
                " AND (instr(message, ?) > 0 OR instr(message, ?) > 0 OR instr(message, ?) > 0)",
            );
            params.push(Value::Text(format!("\"status\":{status}")));
            params.push(Value::Text(format!("\" {status} ")));
            params.push(Value::Text(format!("\"DownstreamStatus\":{status}")));
        }
        if let Some(level) = self.level {
            let mut marks = ERROR_MARKS.to_vec();
            if level == Level::Warning {
                marks.extend(WARNING_MARKS);
            }
            let any = vec!["instr(message, ?) > 0"; marks.len()].join(" OR ");
            sql.push_str(&format!(" AND ({any})"));
            params.extend(marks.iter().map(|m| Value::Text((*m).to_owned())));
        }
        if self.filters_severity() {
            let is_error = vec!["instr(message, ?) > 0"; ERROR_MARKS.len()].join(" OR ");
            let is_warning = vec!["instr(message, ?) > 0"; WARNING_MARKS.len()].join(" OR ");
            let mut parts = Vec::new();
            if self.only.contains(&Severity::Error) {
                parts.push(format!("({is_error})"));
                params.extend(ERROR_MARKS.iter().map(|m| Value::Text((*m).to_owned())));
            }
            if self.only.contains(&Severity::Warning) {
                parts.push(format!("(NOT ({is_error}) AND ({is_warning}))"));
                params.extend(ERROR_MARKS.iter().map(|m| Value::Text((*m).to_owned())));
                params.extend(WARNING_MARKS.iter().map(|m| Value::Text((*m).to_owned())));
            }
            if self.only.contains(&Severity::Info) {
                parts.push(format!("(NOT ({is_error}) AND NOT ({is_warning}))"));
                params.extend(ERROR_MARKS.iter().map(|m| Value::Text((*m).to_owned())));
                params.extend(WARNING_MARKS.iter().map(|m| Value::Text((*m).to_owned())));
            }
            sql.push_str(&format!(" AND ({})", parts.join(" OR ")));
        }
        (sql, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: &[&str] = &[
        "2026-11-17T10:00:01Z INFO login ok user=demo ip=203.0.113.4",
        "2026-11-17T10:00:02Z WARN login failed user=root ip=203.0.113.9",
        "2026-11-17T10:00:03Z ERROR unexpected failure in /boom",
        "{\"level\":\"error\",\"status\":500,\"request\":{\"uri\":\"/boom\"}}",
        "{\"level\":\"info\",\"status\":404,\"request\":{\"uri\":\"/admin\"}}",
        "203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] \"GET /login HTTP/1.1\" 200 5 \"-\" \"x\"",
        "Login OK per Anna",
    ];

    /// Le righe che la ricerca SQL trova.
    fn sql_rows(query: &Query) -> Vec<String> {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE entries(id INTEGER PRIMARY KEY, message TEXT)",
            [],
        )
        .unwrap();
        for line in LINES {
            db.execute("INSERT INTO entries(message) VALUES(?1)", [line])
                .unwrap();
        }
        let (filters, values) = query.sql();
        let mut stmt = db
            .prepare(&format!(
                "SELECT message FROM entries WHERE 1 = 1{filters} ORDER BY id"
            ))
            .unwrap();
        stmt.query_map(rusqlite::params_from_iter(values), |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// SQL (archivio) e `matches` (file originali) devono scegliere le stesse righe:
    /// la cancellazione dagli originali si basa su questo.
    fn same(query: Query) -> Vec<String> {
        let by_sql = sql_rows(&query);
        let by_match: Vec<String> = LINES
            .iter()
            .filter(|l| query.matches(l))
            .map(|l| l.to_string())
            .collect();
        assert_eq!(by_sql, by_match, "{query:?}");
        by_sql
    }

    #[test]
    fn sql_and_matches_agree() {
        assert_eq!(same(Query::default()).len(), LINES.len());
        assert_eq!(same(Query::text("login")).len(), 3);
        let mut q = Query::text("login");
        q.ignore_case = true;
        assert_eq!(same(q).len(), 4);
        assert_eq!(
            same(Query {
                ip: Some("203.0.113.4".into()),
                ..Query::default()
            })
            .len(),
            2
        );
        assert_eq!(
            same(Query {
                status: Some(404),
                ..Query::default()
            })
            .len(),
            1
        );
        assert_eq!(
            same(Query {
                status: Some(200),
                ..Query::default()
            })
            .len(),
            1
        );
        assert_eq!(
            same(Query {
                path: Some("/boom".into()),
                ..Query::default()
            })
            .len(),
            2
        );
        assert_eq!(
            same(Query {
                level: Some(Level::Error),
                ..Query::default()
            })
            .len(),
            2
        );
        assert_eq!(
            same(Query {
                level: Some(Level::Warning),
                ..Query::default()
            })
            .len(),
            3
        );
    }

    #[test]
    fn every_line_has_one_severity() {
        let counts = |s: Severity| LINES.iter().filter(|l| severity(l) == s).count();
        assert_eq!(counts(Severity::Error), 2);
        assert_eq!(counts(Severity::Warning), 1);
        assert_eq!(counts(Severity::Info), 4);
    }

    #[test]
    fn severity_checkboxes() {
        let only = |levels: &[Severity]| Query {
            only: levels.to_vec(),
            ..Query::default()
        };
        assert_eq!(same(only(&[Severity::Error])).len(), 2);
        assert_eq!(same(only(&[Severity::Warning])).len(), 1);
        assert_eq!(same(only(&[Severity::Info])).len(), 4);
        assert_eq!(same(only(&[Severity::Info, Severity::Error])).len(), 6);
        // Tutte e tre, o nessuna: non filtra.
        assert_eq!(
            same(only(&[Severity::Info, Severity::Warning, Severity::Error])).len(),
            7
        );
        assert_eq!(same(only(&[])).len(), 7);
        // Insieme ad altri filtri.
        let mut q = only(&[Severity::Warning]);
        q.text = "login".into();
        assert_eq!(same(q).len(), 1);
    }
}
