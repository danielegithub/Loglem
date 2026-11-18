//! Ricette: quello che serve sapere di un programma (dove scrive i log, in che formato,
//! come riconoscerlo, come leggerne la configurazione), scritto in file JSON dentro
//! `src/recipes/`. Nessun programma è scritto nel codice: per aggiungerne uno basta un file.
use crate::model::LogKind;
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Le ricette incluse nel programma, raccolte da `build.rs`.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/recipes.rs"));

/// Le lingue in cui ogni nota deve esistere (le stesse dei suggerimenti in `src/i18n/`).
pub const LANGUAGES: &[&str] = &["it", "en"];

/// Le categorie del primo passo di «Aggiungi».
pub const CATEGORIES: &[&str] = &[
    "web",
    "database",
    "mail",
    "app",
    "admin",
    "scheduled",
    "system",
];

pub const READERS: &[&str] = &["directives", "caddyfile", "traefik"];

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recognize {
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub configs: Vec<String>,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub units: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogSpec {
    pub path: String,
    pub kind: LogKind,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSpec {
    pub reader: String,
    pub files: Vec<String>,
    /// Direttiva → tipo di log (solo per il lettore `directives`).
    #[serde(default)]
    pub logs: BTreeMap<String, LogKind>,
    /// Direttive con i nomi dei siti (es. `server_name`).
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Parole che aprono il blocco di un sito (es. `server`, `<VirtualHost`).
    #[serde(default)]
    pub blocks: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalSpec {
    #[serde(default)]
    pub units: Vec<String>,
    #[serde(default)]
    pub identifiers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub categories: Vec<String>,
    #[serde(default)]
    pub recognize: Recognize,
    #[serde(default)]
    pub logs: Vec<LogSpec>,
    #[serde(default)]
    pub config: Option<ConfigSpec>,
    #[serde(default)]
    pub journal: Option<JournalSpec>,
    /// Chi ruota i file di solito: `logrotate` o `program`.
    #[serde(default)]
    pub rotation: Option<String>,
    /// Indicazioni brevi, per chiave (`access`, `error`, `rotation`) e lingua.
    #[serde(default)]
    pub notes: BTreeMap<String, BTreeMap<String, String>>,
}

impl Recipe {
    pub fn note(&self, key: &str) -> Option<&str> {
        let notes = self.notes.get(key)?;
        notes
            .get("it")
            .or_else(|| notes.get("en"))
            .map(String::as_str)
    }

    pub fn in_category(&self, category: &str) -> bool {
        self.categories.iter().any(|c| c == category)
    }

    fn validate(&self, file: &str) -> Result<()> {
        ensure!(
            self.id == file,
            "La ricetta {file}.json ha id «{}»",
            self.id
        );
        ensure!(
            !self.id.is_empty()
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "Id non valido: {}",
            self.id
        );
        ensure!(!self.name.trim().is_empty(), "{}: manca il nome", self.id);
        ensure!(
            !self.categories.is_empty(),
            "{}: manca la categoria",
            self.id
        );
        for c in &self.categories {
            ensure!(
                CATEGORIES.contains(&c.as_str()),
                "{}: categoria sconosciuta «{c}»",
                self.id
            );
        }
        ensure!(
            !self.logs.is_empty() || self.config.is_some() || self.journal.is_some(),
            "{}: indica almeno dove sono i log (logs, config o journal)",
            self.id
        );
        for log in &self.logs {
            ensure!(!log.path.is_empty(), "{}: percorso vuoto", self.id);
            if let Some(format) = &log.format {
                ensure!(
                    crate::formats::Format::parse(format).is_some(),
                    "{}: formato sconosciuto «{format}»",
                    self.id
                );
            }
        }
        if let Some(config) = &self.config {
            ensure!(
                READERS.contains(&config.reader.as_str()),
                "{}: lettore sconosciuto «{}»",
                self.id,
                config.reader
            );
            ensure!(
                !config.files.is_empty(),
                "{}: manca il file di configurazione",
                self.id
            );
            ensure!(
                config.reader != "directives" || !config.logs.is_empty(),
                "{}: il lettore «directives» ha bisogno delle direttive dei log",
                self.id
            );
        }
        if let Some(rotation) = &self.rotation {
            ensure!(
                ["logrotate", "program"].contains(&rotation.as_str()),
                "{}: rotazione sconosciuta «{rotation}»",
                self.id
            );
        }
        for (key, notes) in &self.notes {
            ensure!(
                ["access", "error", "rotation"].contains(&key.as_str()),
                "{}: nota sconosciuta «{key}»",
                self.id
            );
            for lang in LANGUAGES {
                ensure!(
                    notes.get(*lang).is_some_and(|n| !n.trim().is_empty()),
                    "{}: alla nota «{key}» manca la lingua «{lang}»",
                    self.id
                );
            }
        }
        Ok(())
    }
}

fn parse_all() -> Result<Vec<Recipe>> {
    let mut all = Vec::new();
    let mut ids = HashSet::new();
    for (file, text) in FILES {
        let recipe: Recipe = serde_json::from_str(text)
            .with_context(|| format!("Ricetta {file}.json non valida"))?;
        recipe.validate(file)?;
        ensure!(
            ids.insert(recipe.id.clone()),
            "Ricetta ripetuta: {}",
            recipe.id
        );
        all.push(recipe);
    }
    all.sort_by_key(|r| r.name.to_lowercase());
    Ok(all)
}

/// Controllo per `self-check` e per i test: ogni ricetta deve essere valida.
pub fn validate_all() -> Result<usize> {
    let all = parse_all()?;
    if all.is_empty() {
        bail!("Nessuna ricetta inclusa");
    }
    Ok(all.len())
}

/// Tutte le ricette, in ordine alfabetico. Sono controllate dai test e da `self-check`.
pub fn all() -> &'static [Recipe] {
    static ALL: OnceLock<Vec<Recipe>> = OnceLock::new();
    ALL.get_or_init(|| parse_all().unwrap_or_default())
}

pub fn get(id: &str) -> Option<&'static Recipe> {
    all().iter().find(|r| r.id == id)
}

// ---------- percorsi ----------

/// `~` diventa la cartella dell'utente; i caratteri jolly diventano i file che esistono.
pub fn expand(pattern: &str) -> Vec<PathBuf> {
    let pattern = match pattern.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest).display().to_string(),
            None => return vec![],
        },
        None => pattern.to_owned(),
    };
    if pattern.contains(['*', '?', '[']) {
        let mut found: Vec<PathBuf> = glob::glob(&pattern)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        found.sort();
        found
    } else {
        vec![PathBuf::from(pattern)]
    }
}

/// I file di configurazione della ricetta che esistono. `LOGLEM_CONFIG_<ID>` (es.
/// `LOGLEM_CONFIG_NGINX`) indica un percorso in più, per installazioni non standard.
pub fn config_files(recipe: &Recipe) -> Vec<PathBuf> {
    let Some(config) = &recipe.config else {
        return vec![];
    };
    let variable = format!(
        "LOGLEM_CONFIG_{}",
        recipe.id.to_uppercase().replace('-', "_")
    );
    std::env::var(variable)
        .into_iter()
        .chain(config.files.iter().cloned())
        .flat_map(|p| expand(&p))
        .filter(|p| p.is_file())
        .collect()
}

// ---------- lettura delle configurazioni ----------

/// Un log indicato nella configurazione di un programma.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigLog {
    pub path: PathBuf,
    pub kind: LogKind,
    /// I nomi di sito del blocco in cui compare (vuoto se è fuori dai blocchi).
    pub hosts: Vec<String>,
    pub from: PathBuf,
    #[serde(skip)]
    pub roll: Option<Roll>,
}

/// Come il programma stesso ruota il file, se la configurazione lo dice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Roll {
    pub keep: Option<u32>,
    pub keep_for_days: Option<f64>,
}

pub fn config_logs(recipe: &Recipe) -> Vec<ConfigLog> {
    let Some(config) = &recipe.config else {
        return vec![];
    };
    let mut found = Vec::new();
    for file in config_files(recipe) {
        let text = match config.reader.as_str() {
            "directives" => crate::discover::with_includes(&file),
            _ => std::fs::read_to_string(&file).unwrap_or_default(),
        };
        let mut logs = match config.reader.as_str() {
            "directives" => crate::discover::directives(&text, config),
            "caddyfile" => crate::discover::caddyfile(&text),
            "traefik" => crate::discover::traefik(&text),
            _ => vec![],
        };
        for log in &mut logs {
            log.from = file.clone();
        }
        found.extend(logs);
    }
    found.dedup_by(|a, b| a.path == b.path && a.kind == b.kind);
    found
}

/// Come il programma ruota un file, se lo dice la sua configurazione.
pub fn rotation_for(path: &Path) -> Option<(&'static Recipe, Roll)> {
    all().iter().find_map(|recipe| {
        config_logs(recipe)
            .into_iter()
            .find(|l| l.path == path)
            .and_then(|l| l.roll.map(|roll| (recipe, roll)))
    })
}

// ---------- che cosa gira su questo server ----------

/// Quello che si vede su questo computer: processi, container e servizi del journal.
#[derive(Debug, Default, Clone)]
pub struct Machine {
    pub processes: HashSet<String>,
    pub containers: Vec<crate::sources::podman::Listed>,
    pub units: HashSet<String>,
}

impl Machine {
    /// Raccoglie tutto in una volta; ogni parte che non risponde resta vuota.
    pub fn scan() -> Self {
        Self {
            processes: processes(),
            containers: crate::sources::podman::list_all(),
            units: crate::sources::journal::units()
                .unwrap_or_default()
                .into_iter()
                .collect(),
        }
    }
}

fn processes() -> HashSet<String> {
    #[cfg(unix)]
    {
        let output = crate::process::output(
            std::process::Command::new("ps").args(["-A", "-o", "comm="]),
            std::time::Duration::from_secs(5),
        )
        .unwrap_or_default();
        String::from_utf8_lossy(&output)
            .lines()
            .map(|l| l.trim().rsplit('/').next().unwrap_or("").to_owned())
            .filter(|l| !l.is_empty())
            .collect()
    }
    #[cfg(not(unix))]
    {
        HashSet::new()
    }
}

/// Il nome breve di un'immagine: `docker.io/library/nginx:1.27` → `nginx`.
pub fn image_name(image: &str) -> &str {
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split(['@', ':']).next().unwrap_or(last)
}

fn image_matches(recipe: &Recipe, image: &str) -> bool {
    let short = image_name(image);
    recipe
        .recognize
        .images
        .iter()
        .any(|i| short == i || short.starts_with(&format!("{i}-")))
}

fn unit_matches(pattern: &str, unit: &str) -> bool {
    pattern == unit
        || pattern
            .strip_suffix(".service")
            .is_some_and(|p| unit.strip_suffix(".service") == Some(p))
}

/// Perché Loglem pensa che questo programma sia presente (vuoto = non trovato).
pub fn found(recipe: &Recipe, machine: &Machine) -> Vec<String> {
    let mut reasons = Vec::new();
    for name in &recipe.recognize.processes {
        if machine
            .processes
            .iter()
            .any(|p| p == name || p.starts_with(name.as_str()))
        {
            reasons.push(format!("processo «{name}» attivo"));
            break;
        }
    }
    for config in recipe.recognize.configs.iter().flat_map(|c| expand(c)) {
        if config.exists() {
            reasons.push(format!("configurazione in {}", config.display()));
            break;
        }
    }
    for container in &machine.containers {
        if image_matches(recipe, &container.image) {
            reasons.push(format!(
                "container «{}» ({})",
                container.name,
                image_name(&container.image)
            ));
            break;
        }
    }
    for unit in &recipe.recognize.units {
        if machine.units.iter().any(|u| unit_matches(unit, u)) {
            reasons.push(format!("servizio {unit}"));
            break;
        }
    }
    reasons
}

// ---------- quale programma scrive un log ----------

fn path_matches(pattern: &str, path: &Path) -> bool {
    let pattern = match pattern.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest).display().to_string(),
            None => return false,
        },
        None => pattern.to_owned(),
    };
    glob::Pattern::new(&pattern).is_ok_and(|p| {
        p.matches_path_with(
            path,
            glob::MatchOptions {
                case_sensitive: true,
                require_literal_separator: true,
                require_literal_leading_dot: false,
            },
        )
    })
}

/// La ricetta del programma che scrive questo file: dal percorso tipico o dalla sua
/// configurazione. Serve solo come etichetta, mai per decidere cosa fare.
pub fn for_path(path: &Path) -> Option<&'static Recipe> {
    all()
        .iter()
        .find(|r| r.logs.iter().any(|l| path_matches(&l.path, path)))
        .or_else(|| {
            all()
                .iter()
                .find(|r| config_logs(r).iter().any(|l| l.path == path))
        })
}

/// La ricetta di un servizio o programma del journal.
pub fn for_journal(units: &[String], identifiers: &[String]) -> Option<&'static Recipe> {
    all().iter().find(|r| {
        let spec = r.journal.clone().unwrap_or_default();
        units.iter().any(|u| {
            spec.units
                .iter()
                .chain(r.recognize.units.iter())
                .any(|p| unit_matches(p, u))
        }) || identifiers.iter().any(|i| spec.identifiers.contains(i))
    })
}

pub fn for_image(image: &str) -> Option<&'static Recipe> {
    all().iter().find(|r| image_matches(r, image))
}

/// Il formato tipico di un log secondo la ricetta (per i file del suo percorso tipico).
pub fn typical(path: &Path) -> Option<(&'static Recipe, &'static LogSpec)> {
    all().iter().find_map(|r| {
        r.logs
            .iter()
            .find(|l| path_matches(&l.path, path))
            .map(|l| (r, l))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recipe_is_valid() {
        let n = validate_all().unwrap();
        assert!(n >= 17, "ricette: {n}");
        assert_eq!(all().len(), n);
        // Tutte le categorie hanno almeno un programma, così il passo 2 non resta vuoto.
        for category in ["web", "database", "mail", "app", "admin", "scheduled"] {
            assert!(all().iter().any(|r| r.in_category(category)), "{category}");
        }
    }

    #[test]
    fn web_servers_say_how_to_turn_on_the_access_log() {
        for recipe in all().iter().filter(|r| r.in_category("web")) {
            assert!(recipe.note("access").is_some(), "{}", recipe.id);
        }
    }

    #[test]
    fn the_same_languages_as_the_suggestions() {
        for lang in LANGUAGES {
            let file = format!("{}/src/i18n/advice.{lang}.json", env!("CARGO_MANIFEST_DIR"));
            assert!(Path::new(&file).is_file(), "manca {file}");
        }
    }

    #[test]
    fn programs_are_recognized_by_path_image_and_unit() {
        assert_eq!(
            for_path(Path::new("/var/log/nginx/access.log")).map(|r| r.id.as_str()),
            Some("nginx")
        );
        assert_eq!(
            for_path(Path::new("/var/log/httpd/error_log")).map(|r| r.id.as_str()),
            Some("apache")
        );
        assert_eq!(
            for_path(Path::new("/var/log/postgresql/postgresql-16-main.log"))
                .map(|r| r.id.as_str()),
            Some("postgresql")
        );
        assert!(for_path(Path::new("/srv/myapp/app.log")).is_none());
        assert_eq!(image_name("docker.io/library/nginx:1.27"), "nginx");
        assert_eq!(
            for_image("docker.io/library/postgres:16").map(|r| r.id.as_str()),
            Some("postgresql")
        );
        assert_eq!(
            for_journal(&["sshd.service".into()], &[]).map(|r| r.id.as_str()),
            Some("sshd")
        );
        assert_eq!(
            for_journal(&[], &["CRON".into()]).map(|r| r.id.as_str()),
            Some("cron")
        );
    }

    #[test]
    fn detection_explains_why_a_program_was_found() {
        let machine = Machine {
            processes: ["nginx".to_owned(), "php-fpm8.3".to_owned()].into(),
            containers: vec![crate::sources::podman::Listed {
                runtime: crate::sources::podman::Runtime::Podman,
                id: "a".repeat(64),
                name: "db".into(),
                image: "docker.io/library/postgres:16".into(),
                project: "shop".into(),
                state: "running".into(),
            }],
            units: ["redis-server.service".to_owned()].into(),
        };
        let reasons = |id: &str| found(get(id).unwrap(), &machine);
        assert!(reasons("nginx")
            .iter()
            .any(|r| r.contains("processo «nginx»")));
        assert!(reasons("php-fpm").iter().any(|r| r.contains("processo")));
        assert!(reasons("postgresql")
            .iter()
            .any(|r| r.contains("container «db»")));
        assert!(reasons("redis")
            .iter()
            .any(|r| r.contains("redis-server.service")));
        assert!(reasons("haproxy").is_empty());
    }
}
