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
            if !has(&format!("\"status\":{status}")) && !has(&format!("\" {status} ")) {
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
        true
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
            sql.push_str(" AND (instr(message, ?) > 0 OR instr(message, ?) > 0)");
            params.push(Value::Text(format!("\"status\":{status}")));
            params.push(Value::Text(format!("\" {status} ")));
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
        (sql, params)
    }
}
