//! Legge le regole di logrotate per dire chi ripulisce un file di log, e ogni quanto.
//! Solo lettura: Loglem non modifica mai la configurazione di logrotate.
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAIN: &str = "/etc/logrotate.conf";
const MAX_DEPTH: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Every {
    Day,
    Week,
    Month,
    Year,
}

impl Every {
    fn days(self) -> u32 {
        match self {
            Every::Day => 1,
            Every::Week => 7,
            Every::Month => 30,
            Every::Year => 365,
        }
    }
    fn text(self) -> &'static str {
        match self {
            Every::Day => "ogni giorno",
            Every::Week => "ogni settimana",
            Every::Month => "ogni mese",
            Every::Year => "ogni anno",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub every: Option<Every>,
    /// `rotate N`: quante copie vecchie restano.
    pub keep: Option<u32>,
    /// `maxage N`: le copie più vecchie di N giorni vengono tolte.
    pub max_age_days: Option<u32>,
}

impl Rule {
    fn set(&mut self, line: &str) {
        let mut words = line.split_whitespace();
        let number = |w: Option<&str>| w.and_then(|v| v.parse::<u32>().ok());
        match words.next() {
            Some("daily") => self.every = Some(Every::Day),
            Some("weekly") => self.every = Some(Every::Week),
            Some("monthly") => self.every = Some(Every::Month),
            Some("yearly") => self.every = Some(Every::Year),
            Some("rotate") => self.keep = number(words.next()).or(self.keep),
            Some("maxage") => self.max_age_days = number(words.next()).or(self.max_age_days),
            _ => {}
        }
    }

    fn over(&self, defaults: &Rule) -> Rule {
        Rule {
            every: self.every.or(defaults.every),
            keep: self.keep.or(defaults.keep),
            max_age_days: self.max_age_days.or(defaults.max_age_days),
        }
    }

    /// Per quanti giorni circa restano le copie vecchie, se la regola lo permette.
    pub fn days(&self) -> Option<u32> {
        let by_count = match (self.every, self.keep) {
            (Some(every), Some(keep)) => Some(keep.saturating_mul(every.days())),
            _ => None,
        };
        match (by_count, self.max_age_days) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(every) = self.every {
            parts.push(format!("apre un nuovo file {}", every.text()));
        }
        match self.keep {
            Some(0) => parts.push("non tiene copie vecchie".into()),
            Some(1) => parts.push("tiene 1 copia".into()),
            Some(n) => parts.push(format!("tiene {n} copie")),
            None => {}
        }
        if let Some(days) = self.max_age_days {
            parts.push(format!("toglie quelle più vecchie di {days} giorni"));
        }
        let mut text = if parts.is_empty() {
            "è configurato".to_owned()
        } else {
            parts.join(", ")
        };
        if let Some(days) = self.days() {
            text.push_str(&format!(", circa {days} giorni"));
        }
        text
    }
}

/// Le regole lette dalla configurazione, in ordine.
pub struct Rules {
    stanzas: Vec<(Vec<String>, Rule)>,
    defaults: Rule,
}

pub enum Lookup {
    Found(Rule),
    /// logrotate c'è ma nessuna regola riguarda questo file.
    NoRule,
    /// Questo computer non ha logrotate (o non si può leggere).
    NotInstalled,
}

impl Rules {
    /// Legge `/etc/logrotate.conf` (o il file indicato da `LOGLEM_LOGROTATE_CONF`).
    pub fn load() -> Option<Rules> {
        let main = std::env::var("LOGLEM_LOGROTATE_CONF").unwrap_or_else(|_| MAIN.to_owned());
        Self::load_from(Path::new(&main))
    }

    pub fn load_from(main: &Path) -> Option<Rules> {
        if !main.is_file() {
            return None;
        }
        let mut rules = Rules {
            stanzas: Vec::new(),
            defaults: Rule::default(),
        };
        rules.read(main, 0);
        Some(rules)
    }

    fn read(&mut self, file: &Path, depth: u8) {
        let Ok(text) = fs::read_to_string(file) else {
            return;
        };
        let mut patterns: Vec<String> = Vec::new();
        let mut current: Option<(Vec<String>, Rule)> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((names, rule)) = current.as_mut() {
                if line.starts_with('}') {
                    let done = (std::mem::take(names), rule.clone());
                    self.stanzas.push(done);
                    current = None;
                } else {
                    rule.set(line);
                }
                continue;
            }
            if line.starts_with('{') {
                // L'apertura può stare da sola su una riga, dopo l'elenco dei file.
                current = Some((std::mem::take(&mut patterns), Rule::default()));
            } else if let Some(target) = line.strip_prefix("include ") {
                if depth < MAX_DEPTH {
                    self.include(Path::new(target.trim()), depth + 1);
                }
            } else if line.starts_with('/') || line.starts_with('"') || line.starts_with('\'') {
                let (before, opens) = match line.split_once('{') {
                    Some((before, _)) => (before, true),
                    None => (line, false),
                };
                patterns.extend(split_patterns(before));
                if opens {
                    current = Some((std::mem::take(&mut patterns), Rule::default()));
                }
            } else {
                self.defaults.set(line);
            }
        }
    }

    fn include(&mut self, target: &Path, depth: u8) {
        if target.is_dir() {
            let Ok(entries) = fs::read_dir(target) else {
                return;
            };
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && !ignored(p))
                .collect();
            files.sort();
            for file in files {
                self.read(&file, depth);
            }
        } else {
            self.read(target, depth);
        }
    }

    pub fn find(&self, path: &Path) -> Lookup {
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        let text = path.to_string_lossy();
        for (names, rule) in &self.stanzas {
            let hit = names.iter().any(|name| {
                name == text.as_ref()
                    || glob::Pattern::new(name).is_ok_and(|p| p.matches_with(&text, options))
            });
            if hit {
                return Lookup::Found(rule.over(&self.defaults));
            }
        }
        Lookup::NoRule
    }
}

/// Estensioni che logrotate stesso ignora nelle cartelle incluse.
fn ignored(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with('.')
        || [
            "~",
            ".rpmsave",
            ".rpmorig",
            ".rpmnew",
            ".dpkg-old",
            ".dpkg-dist",
            ".dpkg-new",
            ".dpkg-bak",
            ".disabled",
            ".swp",
        ]
        .iter()
        .any(|ext| name.ends_with(ext))
}

fn split_patterns(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !word.is_empty() {
                    found.push(std::mem::take(&mut word));
                }
            }
            (None, c) => word.push(c),
        }
    }
    if !word.is_empty() {
        found.push(word);
    }
    found
}

/// Cerca la regola di un file leggendo logrotate da capo (utile quando serve una sola volta).
pub fn lookup(path: &Path) -> Lookup {
    match Rules::load() {
        Some(rules) => rules.find(path),
        None => Lookup::NotInstalled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (tempfile::TempDir, Rules) {
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("logrotate.conf");
        let drop_in = dir.path().join("logrotate.d");
        fs::create_dir(&drop_in).unwrap();
        fs::write(
            &conf,
            format!(
                "# valori generali\nweekly\nrotate 4\ninclude {}\n",
                drop_in.display()
            ),
        )
        .unwrap();
        fs::write(
            drop_in.join("shop"),
            "/var/log/shop/access.log /var/log/shop/error.log {\n    daily\n    rotate 14\n    compress\n}\n",
        )
        .unwrap();
        fs::write(
            drop_in.join("api"),
            "\"/srv/api/logs/*.log\"\n{\n    monthly\n    maxage 60\n}\n",
        )
        .unwrap();
        fs::write(
            drop_in.join("old.dpkg-old"),
            "/var/log/ignored.log {\n daily\n}\n",
        )
        .unwrap();
        let rules = Rules::load_from(&conf).unwrap();
        (dir, rules)
    }

    #[test]
    fn finds_a_rule_by_exact_path_and_describes_it() {
        let (_dir, rules) = sample();
        let Lookup::Found(rule) = rules.find(Path::new("/var/log/shop/access.log")) else {
            panic!("rule not found");
        };
        assert_eq!(rule.every, Some(Every::Day));
        assert_eq!(rule.keep, Some(14));
        assert_eq!(rule.days(), Some(14));
        assert_eq!(
            rule.describe(),
            "apre un nuovo file ogni giorno, tiene 14 copie, circa 14 giorni"
        );
    }

    #[test]
    fn matches_wildcards_and_takes_the_smaller_limit() {
        let (_dir, rules) = sample();
        let Lookup::Found(rule) = rules.find(Path::new("/srv/api/logs/app.log")) else {
            panic!("rule not found");
        };
        // mensile, rotate 4 dalle impostazioni generali: 120 giorni, ma maxage 60 vince.
        assert_eq!(rule.every, Some(Every::Month));
        assert_eq!(rule.keep, Some(4));
        assert_eq!(rule.days(), Some(60));
        // Il carattere * non scavalca le sottocartelle.
        assert!(matches!(
            rules.find(Path::new("/srv/api/logs/deep/app.log")),
            Lookup::NoRule
        ));
    }

    #[test]
    fn reports_no_rule_and_ignores_leftover_package_files() {
        let (_dir, rules) = sample();
        assert!(matches!(
            rules.find(Path::new("/home/user/test/access.log")),
            Lookup::NoRule
        ));
        assert!(matches!(
            rules.find(Path::new("/var/log/ignored.log")),
            Lookup::NoRule
        ));
    }

    #[test]
    fn missing_main_file_means_not_installed() {
        assert!(Rules::load_from(Path::new("/nonexistent/logrotate.conf")).is_none());
    }

    #[test]
    fn splits_quoted_and_plain_paths() {
        assert_eq!(
            split_patterns("\"/a b/x.log\" /c/y.log"),
            vec!["/a b/x.log".to_owned(), "/c/y.log".to_owned()]
        );
    }
}
