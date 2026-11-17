//! Ricerca automatica dei log: legge le configurazioni di Caddy, Nginx e Apache e
//! propone i file trovati, da aggiungere con un clic.
//!
//! È una lettura semplice delle direttive principali, non un interprete completo:
//! variabili, import e percorsi relativi complessi possono sfuggire.
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Configurazioni cercate, nell'ordine in cui le installano i pacchetti più comuni.
const CADDYFILES: &[&str] = &["/etc/caddy/Caddyfile", "/usr/local/etc/Caddyfile"];
const NGINX: &[&str] = &["/etc/nginx/nginx.conf", "/usr/local/etc/nginx/nginx.conf"];
const APACHE: &[&str] = &[
    "/etc/apache2/apache2.conf",
    "/etc/httpd/conf/httpd.conf",
    "/usr/local/etc/httpd/httpd.conf",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    /// `caddy` (con gestione dei file ruotati) oppure `file`.
    pub kind: &'static str,
    pub program: &'static str,
    pub path: PathBuf,
    /// Dove è stato trovato: il file di configurazione.
    pub from: PathBuf,
    pub exists: bool,
}

/// Le impostazioni di rotazione di un log di Caddy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaddyRoll {
    pub keep: Option<u32>,
    pub keep_for_days: Option<f64>,
}

/// Percorsi da controllare: quello indicato nella variabile d'ambiente (installazioni in
/// posti non standard), poi quelli abituali.
fn locations(variable: &str, defaults: &[&str]) -> Vec<String> {
    std::env::var(variable)
        .into_iter()
        .chain(defaults.iter().map(|s| s.to_string()))
        .collect()
}

fn as_str(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

/// Tutti i log trovati nelle configurazioni presenti su questo computer.
pub fn find_all() -> Vec<Found> {
    let caddy = locations("LOGLEM_CADDYFILE", CADDYFILES);
    let nginx = locations("LOGLEM_NGINX_CONF", NGINX);
    let apache = locations("LOGLEM_APACHE_CONF", APACHE);
    let mut found = find_in(&as_str(&caddy), &as_str(&nginx), &as_str(&apache));
    for conf in locations(
        "LOGLEM_TRAEFIK_CONF",
        &[
            "/etc/traefik/traefik.yaml",
            "/etc/traefik/traefik.yml",
            "/etc/traefik/traefik.toml",
            "/etc/traefik/traefik.json",
        ],
    ) {
        let file = Path::new(&conf);
        if !file.is_file() {
            continue;
        }
        let text = read(file);
        let mut access = false;
        let mut path = None;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            path = value["accessLog"]["filePath"].as_str().map(PathBuf::from);
        } else {
            for raw in text.lines() {
                let line = raw.split('#').next().unwrap_or("").trim();
                if line.is_empty() {
                    continue;
                }
                if line.starts_with('[')
                    || (!raw.starts_with(char::is_whitespace) && line.ends_with(':'))
                {
                    access = line.eq_ignore_ascii_case("[accesslog]")
                        || line.eq_ignore_ascii_case("accessLog:");
                } else if access {
                    if let Some((key, value)) = line.split_once([':', '=']) {
                        if key.trim().eq_ignore_ascii_case("filePath") {
                            path = Some(PathBuf::from(
                                value.trim().trim_matches(['"', char::from(39)]),
                            ));
                        }
                    }
                }
            }
        }
        if let Some(path) = path.filter(|p| p.is_absolute()) {
            found.push(entry("file", "Traefik", path, file));
        }
    }
    found
}

pub fn find_in(caddyfiles: &[&str], nginx: &[&str], apache: &[&str]) -> Vec<Found> {
    let mut found = Vec::new();
    for config in existing(caddyfiles) {
        for (path, _) in caddy_logs(&read(&config)) {
            found.push(entry("caddy", "Caddy", path, &config));
        }
    }
    for config in existing(nginx) {
        for path in directive_paths(&with_includes(&config), &["access_log", "error_log"]) {
            found.push(entry("file", "Nginx", path, &config));
        }
    }
    for config in existing(apache) {
        for path in directive_paths(&with_includes(&config), &["CustomLog", "ErrorLog"]) {
            found.push(entry("file", "Apache", path, &config));
        }
    }
    found.dedup_by(|a, b| a.path == b.path);
    found
}

fn entry(kind: &'static str, program: &'static str, path: PathBuf, from: &Path) -> Found {
    Found {
        kind,
        program,
        exists: path.is_file(),
        path,
        from: from.to_owned(),
    }
}

fn existing(paths: &[&str]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

// ---------- Caddy ----------

/// I log `output file …` di un Caddyfile, con le impostazioni di rotazione.
pub fn caddy_logs(caddyfile: &str) -> Vec<(PathBuf, CaddyRoll)> {
    let mut logs = Vec::new();
    let lines: Vec<&str> = caddyfile
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .collect();
    for (i, line) in lines.iter().enumerate() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() < 3 || words[0] != "output" || words[1] != "file" {
            continue;
        }
        let mut roll = CaddyRoll::default();
        if words.last() == Some(&"{") {
            // Le opzioni roll_* seguono fino alla graffa di chiusura.
            for option in lines[i + 1..].iter().take_while(|l| !l.starts_with('}')) {
                let parts: Vec<&str> = option.split_whitespace().collect();
                match parts.as_slice() {
                    ["roll_keep", n] => roll.keep = n.parse().ok(),
                    ["roll_keep_for", d] => roll.keep_for_days = parse_duration_days(d),
                    _ => {}
                }
            }
        }
        logs.push((PathBuf::from(words[2].trim_matches('"')), roll));
    }
    logs
}

/// `60d`, `720h`, `90m` → giorni.
fn parse_duration_days(text: &str) -> Option<f64> {
    let (number, unit) = text.split_at(text.find(|c: char| c.is_alphabetic())?);
    let n: f64 = number.parse().ok()?;
    Some(match unit {
        "d" => n,
        "h" => n / 24.0,
        "m" => n / 1440.0,
        _ => return None,
    })
}

/// Impostazioni di rotazione con cui Caddy scrive questo log, se il Caddyfile lo dice.
pub fn caddy_roll_for(log: &Path) -> Option<CaddyRoll> {
    caddy_roll_in(&as_str(&locations("LOGLEM_CADDYFILE", CADDYFILES)), log)
}

pub fn caddy_roll_in(caddyfiles: &[&str], log: &Path) -> Option<CaddyRoll> {
    existing(caddyfiles)
        .iter()
        .flat_map(|config| caddy_logs(&read(config)))
        .find(|(path, _)| path == log)
        .map(|(_, roll)| roll)
}

/// Caddy cancella da sé i file ruotati oltre questi limiti: se il margine è stretto,
/// un fermo di Loglem può far perdere file non ancora letti.
pub fn caddy_margin_warning(roll: &CaddyRoll, unread_rotated: usize) -> Option<String> {
    // Valori predefiniti di Caddy: 10 file, 90 giorni.
    let keep = roll.keep.unwrap_or(10);
    let keep_for = roll.keep_for_days.unwrap_or(90.0);
    if keep_for < 7.0 {
        return Some(format!(
            "Caddy cancella i file ruotati dopo {keep_for:.1} giorni (roll_keep_for): se Loglem \
             resta fermo più a lungo, si perdono righe. Consigliati almeno 7 giorni"
        ));
    }
    if unread_rotated + 1 >= keep as usize {
        return Some(format!(
            "Ci sono {unread_rotated} file ruotati non ancora letti e Caddy ne tiene al massimo \
             {keep} (roll_keep): al prossimo giro rischia di cancellarne uno. Aumenta roll_keep"
        ));
    }
    None
}

// ---------- Nginx e Apache ----------

/// Il file di configurazione più quelli richiamati con `include`/`Include` (un livello di
/// profondità, con i caratteri jolly `*`).
fn with_includes(config: &Path) -> String {
    let text = read(config);
    let base = config.parent().unwrap_or(Path::new("/"));
    let mut all = text.clone();
    for line in text.lines() {
        let words: Vec<&str> = line
            .trim()
            .trim_end_matches(';')
            .split_whitespace()
            .collect();
        if let [directive, pattern, ..] = words.as_slice() {
            if directive.eq_ignore_ascii_case("include") || *directive == "IncludeOptional" {
                let pattern = if pattern.starts_with('/') {
                    pattern.to_string()
                } else {
                    base.join(pattern).display().to_string()
                };
                for path in glob::glob(&pattern).into_iter().flatten().flatten() {
                    all.push('\n');
                    all.push_str(&read(&path));
                }
            }
        }
    }
    all
}

/// Il primo argomento delle direttive indicate, se è un percorso assoluto
/// (`off`, `syslog:` e pipe come `|/usr/bin/rotatelogs` vengono saltati).
fn directive_paths(text: &str, directives: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches(';');
        let mut words = line.split_whitespace();
        let (Some(directive), Some(value)) = (words.next(), words.next()) else {
            continue;
        };
        let value = value.trim_matches('"');
        if directives.contains(&directive)
            && value.starts_with('/')
            && !paths.iter().any(|p: &PathBuf| p == Path::new(value))
        {
            paths.push(PathBuf::from(value));
        }
    }
    paths
}
