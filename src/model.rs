use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<Retention>,
    #[serde(default)]
    pub log_kind: LogKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_dir: Option<PathBuf>,
    #[serde(default)]
    pub detached: bool,
    /// Archivia ed elimina i file ruotati (es. Caddy) dopo averli verificati.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove_rotated: bool,
    pub retention_days: u32,
    pub enabled: bool,
    #[serde(flatten)]
    pub input: Input,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    Access,
    #[default]
    Generic,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RetentionUnit {
    Days,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    pub amount: u32,
    pub unit: RetentionUnit,
}

impl Retention {
    pub fn validate(&self) -> anyhow::Result<()> {
        let max = match self.unit {
            RetentionUnit::Days => 36500,
            RetentionUnit::Months => 1200,
            RetentionUnit::Years => 100,
        };
        anyhow::ensure!(
            (1..=max).contains(&self.amount),
            "Durata non valida (massimo 100 anni, 1200 mesi o 36500 giorni)"
        );
        Ok(())
    }
    pub fn cutoff(&self, now: i64) -> i64 {
        let date = chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(now);
        let past = match self.unit {
            RetentionUnit::Days => {
                date.checked_sub_signed(chrono::Duration::days(i64::from(self.amount)))
            }
            RetentionUnit::Months => date.checked_sub_months(chrono::Months::new(self.amount)),
            RetentionUnit::Years => {
                date.checked_sub_months(chrono::Months::new(self.amount.saturating_mul(12)))
            }
        };
        past.and_then(|d| d.timestamp_nanos_opt())
            .unwrap_or(i64::MIN)
    }
}
impl std::fmt::Display for Retention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unit = match (self.unit, self.amount == 1) {
            (RetentionUnit::Days, true) => "giorno",
            (RetentionUnit::Days, false) => "giorni",
            (RetentionUnit::Months, true) => "mese",
            (RetentionUnit::Months, false) => "mesi",
            (RetentionUnit::Years, true) => "anno",
            (RetentionUnit::Years, false) => "anni",
        };
        write!(f, "{} {unit}", self.amount)
    }
}
impl Source {
    pub fn period(&self) -> Retention {
        self.retention.unwrap_or(Retention {
            amount: self.retention_days,
            unit: RetentionUnit::Days,
        })
    }
    pub fn archive_path(&self, dir: &std::path::Path) -> PathBuf {
        self.archive_dir
            .clone()
            .unwrap_or_else(|| dir.join("archives"))
            .join(format!("{}.db", self.id))
    }
}
impl Default for Source {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: None,
            retention: None,
            log_kind: LogKind::Generic,
            archive_dir: None,
            completed_dir: None,
            detached: false,
            remove_rotated: false,
            retention_days: 28,
            enabled: true,
            input: Input::Folder {
                path: PathBuf::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Input {
    Folder {
        path: PathBuf,
    },
    File {
        path: PathBuf,
        /// Optional, same-directory pattern for uncompressed rotated files.
        rotated_pattern: Option<String>,
    },
    Podman {
        /// Immutable container ID, not a name that could identify a replacement.
        container: String,
        connection: Option<String>,
    },
    /// Journal di systemd, con filtri facoltativi.
    Journal(crate::sources::journal::Filter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSettings {
    pub repository: String,
    /// Base64-encoded, raw 32-byte Ed25519 public key.
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub poll_seconds: u64,
    pub sources: Vec<Source>,
    pub updates: Option<UpdateSettings>,
    /// Porta locale dell'interfaccia web; scelta al primo avvio e poi riusata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_port: Option<u16>,
    /// Accesso da altri computer: indirizzo e utenti.
    #[serde(default, skip_serializing_if = "crate::access::Access::is_default")]
    pub access: crate::access::Access,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            poll_seconds: 5,
            sources: vec![],
            updates: None,
            web_port: None,
            access: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub file_id: Option<String>,
    pub generation: i64,
    pub offset: i64,
    pub event_ns: i64,
    pub collected_ns: i64,
    pub timestamp_origin: &'static str,
    pub raw: Vec<u8>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Cursor {
    pub offset: u64,
    pub generation: i64,
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRow {
    pub source: String,
    pub timestamp: String,
    pub timestamp_origin: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub input: Input,
    pub archive_dir: Option<PathBuf>,
    pub label: String,
    pub retention: Retention,
    pub log_kind: LogKind,
    pub detached: bool,
    pub completed_dir: Option<PathBuf>,
    pub remove_rotated: bool,
    pub warning: Option<String>,
    pub phase: String,
    pub error: Option<String>,
    pub source: String,
    pub enabled: bool,
    pub origin: String,
    pub archive: String,
    pub original_bytes: Option<u64>,
    pub archive_bytes: u64,
    pub entries: i64,
    pub earliest: Option<String>,
    pub last_collection: Option<String>,
    pub message: String,
    pub retention_days: u32,
}
