//! Ricerca automatica dei log: per ogni ricetta (`src/recipes/`) legge i file di
//! configurazione indicati e propone i log che vi compaiono. Qui non c'è nessun programma
//! scritto nel codice: i nomi delle direttive e dei file arrivano dalle ricette.
//!
//! È una lettura semplice delle direttive principali, non un interprete completo:
//! variabili, import e percorsi relativi complessi possono sfuggire.
use crate::{
    model::LogKind,
    recipes::{self, ConfigLog, ConfigSpec, Roll},
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    /// Il programma che lo scrive (nome della ricetta), solo come etichetta.
    pub program: String,
    pub recipe: String,
    pub path: PathBuf,
    pub log_kind: LogKind,
    /// Dove è stato trovato: il file di configurazione.
    pub from: PathBuf,
    pub exists: bool,
}

/// Tutti i log trovati nelle configurazioni presenti su questo computer.
pub fn find_all() -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    for recipe in recipes::all() {
        for log in recipes::config_logs(recipe) {
            if found.iter().any(|f| f.path == log.path) {
                continue;
            }
            found.push(Found {
                program: recipe.name.clone(),
                recipe: recipe.id.clone(),
                exists: log.path.is_file(),
                log_kind: log.kind,
                path: log.path,
                from: log.from,
            });
        }
    }
    found
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Il file di configurazione più quelli richiamati con `include`/`Include` (un livello di
/// profondità, con i caratteri jolly `*`).
pub(crate) fn with_includes(config: &Path) -> String {
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
            let directive = directive.to_ascii_lowercase();
            if directive == "include"
                || directive == "includeoptional"
                || directive == "!includedir"
            {
                let pattern = pattern.trim_matches('"');
                let pattern = if pattern.starts_with('/') {
                    pattern.to_string()
                } else {
                    base.join(pattern).display().to_string()
                };
                let pattern = if directive == "!includedir" {
                    format!("{pattern}/*")
                } else {
                    pattern
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

/// Variabili d'ambiente comuni nei file di configurazione, con il loro valore abituale.
fn expand_variables(value: &str) -> String {
    value
        .replace("${APACHE_LOG_DIR}", "/var/log/apache2")
        .replace("$APACHE_LOG_DIR", "/var/log/apache2")
}

/// Toglie dal nome di un sito schema, porta e punto finale: `https://Example.com:443/` → `example.com`.
pub fn clean_host(host: &str) -> String {
    let host = host
        .trim()
        .trim_matches(['"', '\'', ',', ';'])
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = host.split('/').next().unwrap_or(host);
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// Lettore generico «direttiva valore»: anche `chiave = valore` e `chiave=valore`. I blocchi
/// (es. `server {` o `<VirtualHost`) separano i siti: un log prende i nomi del suo blocco.
pub(crate) fn directives(text: &str, spec: &ConfigSpec) -> Vec<ConfigLog> {
    struct Pending {
        path: PathBuf,
        kind: LogKind,
    }
    let mut out = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    let flush = |hosts: &mut Vec<String>, pending: &mut Vec<Pending>, out: &mut Vec<ConfigLog>| {
        for p in pending.drain(..) {
            out.push(ConfigLog {
                path: p.path,
                kind: p.kind,
                hosts: hosts.clone(),
                from: PathBuf::new(),
                roll: None,
            });
        }
        hosts.clear();
    };
    for raw in text.lines() {
        let line = raw
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches(';');
        if line.is_empty() {
            continue;
        }
        let starts_block = spec.blocks.iter().any(|b| {
            line.strip_prefix(b.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '{', '>', '\t']))
        });
        if starts_block {
            flush(&mut hosts, &mut pending, &mut out);
            continue;
        }
        let (key, rest) = match line.split_once(['=', ' ', '\t']) {
            Some((k, r)) => (k.trim(), r.trim().trim_start_matches('=').trim()),
            None => continue,
        };
        if spec.hosts.iter().any(|h| h.eq_ignore_ascii_case(key)) {
            hosts.extend(
                rest.split_whitespace()
                    .map(clean_host)
                    .filter(|h| !h.is_empty() && h != "_" && h != "localhost"),
            );
            continue;
        }
        let Some(kind) = spec
            .logs
            .iter()
            .find(|(d, _)| d.eq_ignore_ascii_case(key))
            .map(|(_, k)| *k)
        else {
            continue;
        };
        let value = expand_variables(
            rest.split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(['"', '\'']),
        );
        // `off`, `syslog:` e pipe come `|/usr/bin/rotatelogs` non sono file.
        if value.starts_with('/') && !pending.iter().any(|p| p.path == Path::new(&value)) {
            pending.push(Pending {
                path: PathBuf::from(value),
                kind,
            });
        }
    }
    flush(&mut hosts, &mut pending, &mut out);
    out
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

/// Lettore dei file a blocchi con l'indirizzo del sito in testa (formato Caddyfile):
/// `example.com { log { output file /percorso { roll_keep 10 } } }`.
pub(crate) fn caddyfile(text: &str) -> Vec<ConfigLog> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .collect();
    let mut logs = Vec::new();
    let mut depth = 0i32;
    let mut hosts: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if depth == 0 && line.ends_with('{') {
            let head = line.trim_end_matches('{').trim();
            // `{` da solo = opzioni globali; `(nome)` = frammento riusabile.
            hosts = if head.is_empty() || head.starts_with('(') {
                vec![]
            } else {
                head.split([',', ' '])
                    .map(clean_host)
                    .filter(|h| !h.is_empty())
                    .collect()
            };
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() >= 3 && words[0] == "output" && words[1] == "file" {
            let mut roll = Roll::default();
            if words.last() == Some(&"{") {
                for option in lines[i + 1..].iter().take_while(|l| !l.starts_with('}')) {
                    match option.split_whitespace().collect::<Vec<_>>().as_slice() {
                        ["roll_keep", n] => roll.keep = n.parse().ok(),
                        ["roll_keep_for", d] => roll.keep_for_days = parse_duration_days(d),
                        _ => {}
                    }
                }
            }
            logs.push(ConfigLog {
                path: PathBuf::from(words[2].trim_matches('"')),
                kind: LogKind::Access,
                hosts: hosts.clone(),
                from: PathBuf::new(),
                roll: Some(roll),
            });
        }
        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if depth <= 0 {
            depth = 0;
        }
    }
    logs
}

/// Lettore della configurazione statica in YAML, TOML o JSON con le sezioni `accessLog` e
/// `log` (formato Traefik).
pub(crate) fn traefik(text: &str) -> Vec<ConfigLog> {
    let mut found: Vec<(LogKind, String)> = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        for (section, kind) in [("accessLog", LogKind::Access), ("log", LogKind::Error)] {
            if let Some(path) = value[section]["filePath"].as_str() {
                found.push((kind, path.to_owned()));
            }
        }
    } else {
        let mut section: Option<LogKind> = None;
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[')
                || (!raw.starts_with(char::is_whitespace) && line.ends_with(':'))
            {
                let name = line.trim_matches(['[', ']', ':']).to_ascii_lowercase();
                section = match name.as_str() {
                    "accesslog" => Some(LogKind::Access),
                    "log" => Some(LogKind::Error),
                    _ => None,
                };
            } else if let Some(kind) = section {
                if let Some((key, value)) = line.split_once([':', '=']) {
                    if key.trim().eq_ignore_ascii_case("filePath") {
                        found.push((kind, value.trim().trim_matches(['"', '\'']).to_owned()));
                    }
                }
            }
        }
    }
    found
        .into_iter()
        .filter(|(_, p)| p.starts_with('/'))
        .map(|(kind, path)| ConfigLog {
            path: PathBuf::from(path),
            kind,
            hosts: vec![],
            from: PathBuf::new(),
            roll: None,
        })
        .collect()
}

/// Il programma cancella da sé i file ruotati oltre questi limiti: se il margine è stretto,
/// un fermo di Loglem può far perdere file non ancora letti.
pub fn roll_margin_warning(program: &str, roll: &Roll, unread_rotated: usize) -> Option<String> {
    if let Some(keep_for) = roll.keep_for_days.filter(|d| *d < 7.0) {
        return Some(format!(
            "{program} cancella i file ruotati dopo {keep_for:.1} giorni: se Loglem resta fermo \
             più a lungo, si perdono righe. Consigliati almeno 7 giorni"
        ));
    }
    if let Some(keep) = roll.keep.filter(|k| unread_rotated + 1 >= *k as usize) {
        return Some(format!(
            "Ci sono {unread_rotated} file ruotati non ancora letti e {program} ne tiene al massimo \
             {keep}: al prossimo giro rischia di cancellarne uno. Aumenta quel limite"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> ConfigSpec {
        recipes::get(id).unwrap().config.clone().unwrap()
    }

    #[test]
    fn directive_files_give_paths_with_their_site_names() {
        let text = "http {\n  access_log /var/log/nginx/access.log;\n  server {\n    server_name shop.example www.shop.example;\n    access_log /var/log/nginx/shop.access.log combined;\n    error_log /var/log/nginx/shop.error.log;\n  }\n  server {\n    server_name api.example;\n    access_log off;\n    access_log syslog:server=unix:/dev/log;\n  }\n}\n";
        let logs = directives(text, &spec("nginx"));
        let shop: Vec<_> = logs
            .iter()
            .filter(|l| l.hosts.contains(&"shop.example".to_owned()))
            .collect();
        assert_eq!(shop.len(), 2);
        assert!(shop
            .iter()
            .any(|l| l.kind == LogKind::Access && l.path.ends_with("shop.access.log")));
        assert!(shop.iter().any(|l| l.kind == LogKind::Error));
        assert!(logs
            .iter()
            .any(|l| l.path.ends_with("nginx/access.log") && l.hosts.is_empty()));
        assert_eq!(logs.len(), 3, "off e syslog non sono file");
    }

    #[test]
    fn virtual_hosts_and_key_value_files() {
        let text = "<VirtualHost *:443>\n  ServerName blog.example\n  ServerAlias www.blog.example\n  CustomLog ${APACHE_LOG_DIR}/blog-access.log combined\n</VirtualHost>\n";
        let logs = directives(text, &spec("apache"));
        assert_eq!(
            logs[0].path,
            PathBuf::from("/var/log/apache2/blog-access.log")
        );
        assert_eq!(
            logs[0].hosts,
            vec!["blog.example".to_owned(), "www.blog.example".to_owned()]
        );
        let kv = directives(
            "[mysqld]\nlog_error = /var/log/mysql/error.log\n",
            &spec("mysql"),
        );
        assert_eq!(kv[0].path, PathBuf::from("/var/log/mysql/error.log"));
        let quoted = directives(
            "accesslog.filename = \"/var/log/lighttpd/access.log\"\n",
            &spec("lighttpd"),
        );
        assert_eq!(quoted[0].kind, LogKind::Access);
    }

    #[test]
    fn site_blocks_with_the_address_first() {
        let text = "{\n  email admin@example.com\n}\nshop.example, www.shop.example {\n  log {\n    output file /var/log/caddy/shop.log {\n      roll_keep 10\n      roll_keep_for 14d\n    }\n  }\n}\n(common) {\n  log\n}\n";
        let logs = caddyfile(text);
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].hosts,
            vec!["shop.example".to_owned(), "www.shop.example".to_owned()]
        );
        assert_eq!(logs[0].roll.as_ref().unwrap().keep, Some(10));
        assert_eq!(logs[0].roll.as_ref().unwrap().keep_for_days, Some(14.0));
    }

    #[test]
    fn static_configuration_in_yaml_toml_and_json() {
        let yaml = "accessLog:\n  filePath: /var/log/proxy/access.log\nlog:\n  filePath: \"/var/log/proxy/proxy.log\"\n";
        let toml = "[accessLog]\n  filePath = \"/var/log/proxy/access.log\"\n";
        let json = r#"{"accessLog":{"filePath":"/var/log/proxy/access.log"}}"#;
        assert_eq!(traefik(yaml).len(), 2);
        assert_eq!(traefik(toml)[0].kind, LogKind::Access);
        assert_eq!(
            traefik(json)[0].path,
            PathBuf::from("/var/log/proxy/access.log")
        );
    }

    #[test]
    fn hosts_are_cleaned() {
        assert_eq!(clean_host("https://Shop.Example:443/"), "shop.example");
        assert_eq!(clean_host("\"blog.example\";"), "blog.example");
    }
}
