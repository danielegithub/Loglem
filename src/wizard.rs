//! Il percorso guidato «Aggiungi», uguale per pagina web e terminale:
//! 1. cosa vuoi aggiungere · 2. chi lo fa funzionare · 3. dove sono i log · 4. riepilogo.
//!
//! Ogni passo parte da quello che Loglem trova su questo server; quello che si può decidere
//! con certezza arriva già scelto, il resto si sceglie a mano. Nessun programma è scritto
//! qui: i programmi arrivano dalle ricette (`src/recipes/`).
use crate::{
    config,
    management::NewSource,
    model::{LogKind, Retention, SERVER_SITE},
    recipes::{self, Machine, Recipe},
    sources::{self, journal::Filter, podman::Listed},
};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Una scheda del passo 1.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Category {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Il nome scelto diventa un sito (gli altri log vanno in «Questo server»).
    pub names_site: bool,
    /// Il passo 2 («chi lo fa funzionare?») serve a trovare i log.
    pub asks_program: bool,
    /// La categoria delle ricette da proporre.
    #[serde(skip)]
    pub recipes: Option<&'static str>,
    pub kind: LogKind,
    pub name_hint: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category {
        id: "web",
        title: "Un sito web",
        description: "Pagine per i visitatori, es. example.com",
        names_site: true,
        asks_program: true,
        recipes: Some("web"),
        kind: LogKind::Access,
        name_hint: "es. example.com",
    },
    Category {
        id: "service",
        title: "Un servizio web o un'API",
        description: "Risponde a programmi o app, es. api.example.com",
        names_site: true,
        asks_program: true,
        recipes: Some("web"),
        kind: LogKind::Access,
        name_hint: "es. api.example.com",
    },
    Category {
        id: "app",
        title: "Un'applicazione o un progetto",
        description: "Un backend, un worker, un programma scritto da te",
        names_site: true,
        asks_program: false,
        recipes: Some("app"),
        kind: LogKind::Application,
        name_hint: "es. Negozio, worker-email",
    },
    Category {
        id: "database",
        title: "Un database",
        description: "I log del server del database",
        names_site: false,
        asks_program: true,
        recipes: Some("database"),
        kind: LogKind::Database,
        name_hint: "es. Database del negozio",
    },
    Category {
        id: "admin",
        title: "Gli accessi al server",
        description: "SSH, sudo, pannelli di amministrazione",
        names_site: false,
        asks_program: false,
        recipes: Some("admin"),
        kind: LogKind::AdminAccess,
        name_hint: "",
    },
    Category {
        id: "scheduled",
        title: "Le attività pianificate",
        description: "cron, timer",
        names_site: false,
        asks_program: false,
        recipes: Some("scheduled"),
        kind: LogKind::System,
        name_hint: "",
    },
    Category {
        id: "mail",
        title: "La posta",
        description: "Server di posta in uscita o in arrivo",
        names_site: false,
        asks_program: true,
        recipes: Some("mail"),
        kind: LogKind::Other,
        name_hint: "",
    },
    Category {
        id: "system",
        title: "Il sistema",
        description: "Kernel, servizi, avvio",
        names_site: false,
        asks_program: false,
        recipes: Some("system"),
        kind: LogKind::System,
        name_hint: "",
    },
    Category {
        id: "other",
        title: "Un altro log",
        description: "Qualsiasi file o container, senza domande",
        names_site: false,
        asks_program: false,
        recipes: None,
        kind: LogKind::Application,
        name_hint: "",
    },
];

pub fn category(id: &str) -> Result<&'static Category> {
    CATEGORIES
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| anyhow::anyhow!("Scelta sconosciuta: {id}"))
}

// ---------- passo 2: chi lo fa funzionare ----------

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub id: String,
    pub name: String,
    /// Perché Loglem pensa che sia su questo server (vuoto = non trovato).
    pub found: Vec<String>,
    /// Il nome del sito compare nella sua configurazione.
    pub has_site: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Programs {
    pub programs: Vec<Program>,
    /// Già scelto quando c'è un solo programma trovato e il sito è nella sua configurazione.
    pub suggested: Option<String>,
    pub question: Option<String>,
}

/// Le varianti di un nome da cercare in percorsi, container e servizi:
/// `example.com` → `example.com`, `example_com`, `example-com`, `example`.
pub fn variants(name: &str) -> Vec<String> {
    const GENERIC: &[&str] = &[
        "www", "api", "app", "web", "mail", "admin", "test", "dev", "log", "logs", "site",
        "staging", "prod", "server",
    ];
    let name = name.trim().to_lowercase();
    let name = name.strip_prefix("www.").unwrap_or(&name).to_owned();
    if name.len() < 3 {
        return vec![];
    }
    let mut out = vec![name.clone()];
    let slug = |s: &str, sep: &str| {
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(sep)
    };
    out.push(slug(&name, "_"));
    out.push(slug(&name, "-"));
    if let Some((first, rest)) = name.split_once('.') {
        // Il nome senza dominio di primo livello: «example.com» → «example».
        let without_tld = rest
            .rsplit_once('.')
            .map_or(first.to_owned(), |(middle, _)| format!("{first}.{middle}"));
        for v in [
            without_tld.clone(),
            slug(&without_tld, "-"),
            slug(&without_tld, "_"),
        ] {
            if v.len() >= 3 && !GENERIC.contains(&v.as_str()) {
                out.push(v);
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|v| v.len() >= 3 && seen.insert(v.clone()));
    out
}

fn host_matches(host: &str, name: &str) -> bool {
    let name = name.trim().to_lowercase();
    let bare = name.strip_prefix("www.").unwrap_or(&name);
    let host = host.strip_prefix("www.").unwrap_or(host);
    host == bare
        || host
            .strip_prefix("*.")
            .is_some_and(|domain| bare.ends_with(&format!(".{domain}")))
}

pub fn programs(category_id: &str, name: &str, machine: &Machine) -> Result<Programs> {
    let category = category(category_id)?;
    let Some(group) = category.recipes else {
        return Ok(Programs {
            programs: vec![],
            suggested: None,
            question: None,
        });
    };
    let mut list: Vec<Program> = recipes::all()
        .iter()
        .filter(|r| r.in_category(group))
        .map(|r| Program {
            id: r.id.clone(),
            name: r.name.clone(),
            found: recipes::found(r, machine),
            has_site: !name.trim().is_empty()
                && recipes::config_logs(r)
                    .iter()
                    .any(|l| l.hosts.iter().any(|h| host_matches(h, name))),
        })
        .collect();
    // Prima quelli trovati su questo server, poi in ordine alfabetico.
    list.sort_by_key(|p| (p.found.is_empty(), p.name.to_lowercase()));
    let found: Vec<_> = list.iter().filter(|p| !p.found.is_empty()).collect();
    let (suggested, question) = match found.as_slice() {
        [only] if only.has_site => (
            Some(only.id.clone()),
            Some(format!(
                "Ho trovato {} e il sito nella sua configurazione: è lui?",
                only.name
            )),
        ),
        _ => (None, None),
    };
    Ok(Programs {
        programs: list,
        suggested,
        question,
    })
}

// ---------- passo 3: dove sono i log ----------

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub key: String,
    /// `file`, `folder`, `container`, `pod` o `journal`.
    pub location: &'static str,
    pub title: String,
    pub reason: String,
    /// Corrispondenza certa: la casella arriva già spuntata.
    pub exact: bool,
    pub kind: LogKind,
    pub program: Option<String>,
    pub program_name: Option<String>,
    pub format: Option<String>,
    pub bytes: Option<u64>,
    pub preview: Vec<String>,
    pub readable: bool,
    /// Perché non si legge e cosa fare.
    pub problem: Option<String>,
    pub source: NewSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct Proposals {
    pub proposals: Vec<Proposal>,
    /// Cosa manca (es. il log degli accessi) e come rimediare.
    pub warnings: Vec<String>,
}

const SCAN_DIRS: &[&str] = &["/var/log", "/srv", "/var/www", "/opt"];
const SCAN_LIMIT: usize = 20_000;
const PREVIEW_LIMIT: usize = 8;

/// Le ultime righe di un file (al massimo 3, ognuna accorciata).
fn tail_lines(path: &Path, lines: usize) -> std::io::Result<Vec<String>> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(32 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(32 * 1024).read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let mut all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if start > 0 && !all.is_empty() {
        all.remove(0); // La prima riga può essere tagliata a metà.
    }
    Ok(all
        .iter()
        .rev()
        .take(lines)
        .rev()
        .map(|l| l.chars().take(200).collect())
        .collect())
}

fn sample_format(lines: &[String]) -> Option<String> {
    (!lines.is_empty()).then(|| {
        crate::formats::detect(lines.iter().map(String::as_str))
            .label()
            .to_owned()
    })
}

#[cfg(unix)]
fn user_name(uid: u32) -> String {
    // SAFETY: getpwuid restituisce un puntatore valido fino alla chiamata successiva.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return uid.to_string();
        }
        std::ffi::CStr::from_ptr((*pw).pw_name)
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(unix)]
fn group_name(gid: u32) -> String {
    // SAFETY: getgrgid restituisce un puntatore valido fino alla chiamata successiva.
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            return gid.to_string();
        }
        std::ffi::CStr::from_ptr((*gr).gr_name)
            .to_string_lossy()
            .into_owned()
    }
}

/// Perché un file non si legge e cosa fare, in modo generico.
pub fn unreadable_help(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = fs::metadata(path) {
            // SAFETY: getuid non può fallire.
            let me = user_name(unsafe { libc::getuid() });
            let group = group_name(meta.gid());
            return format!(
                "Non riesco a leggerlo: è di {}:{group}, permesso {:o}, e l'utente che esegue Loglem ({me}) non è nel gruppo {group}. Ti propongo: `sudo usermod -aG {group} {me}`, poi riavvia Loglem. Verifica prima che quel gruppo non dia accesso a più di quanto vuoi.",
                user_name(meta.uid()),
                meta.mode() & 0o777,
            );
        }
    }
    format!(
        "Non riesco a leggere {}: controlla i permessi e il percorso.",
        path.display()
    )
}

struct Builder<'a> {
    out: Vec<Proposal>,
    keys: HashSet<String>,
    previews: usize,
    category: &'a Category,
}

impl Builder<'_> {
    fn has(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    fn push(&mut self, mut p: Proposal) {
        if self.has(&p.key) {
            // Già proposto: tiene il motivo più forte.
            if p.exact {
                if let Some(old) = self.out.iter_mut().find(|o| o.key == p.key) {
                    old.exact = true;
                    old.reason = p.reason;
                }
            }
            return;
        }
        if p.program_name.is_none() {
            if let Some(r) = p.program.as_deref().and_then(recipes::get) {
                p.program_name = Some(r.name.clone());
            }
        }
        self.keys.insert(p.key.clone());
        self.out.push(p);
    }

    fn file(
        &mut self,
        path: &Path,
        kind: LogKind,
        recipe: Option<&Recipe>,
        reason: String,
        exact: bool,
    ) {
        let key = format!("file:{}", path.display());
        if self.has(&key) && !exact {
            return;
        }
        let (readable, problem, preview) = match tail_lines(path, 50) {
            Ok(lines) => (true, None, lines),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                (false, Some(unreadable_help(path)), vec![])
            }
            Err(_) => return,
        };
        let typical = recipes::typical(path);
        let format = sample_format(&preview).or_else(|| {
            typical
                .and_then(|(_, l)| l.format.as_deref())
                .and_then(crate::formats::Format::parse)
                .map(|f| f.label().to_owned())
        });
        let recipe = recipe.or(typical.map(|(r, _)| r));
        self.push(Proposal {
            key,
            location: "file",
            title: path.display().to_string(),
            reason,
            exact: exact && readable,
            kind,
            program: recipe.map(|r| r.id.clone()),
            program_name: None,
            format,
            bytes: fs::metadata(path).ok().map(|m| m.len()),
            preview: preview.iter().rev().take(3).rev().cloned().collect(),
            readable,
            problem,
            source: NewSource {
                kind: "file".into(),
                path: Some(path.to_owned()),
                log_kind: kind,
                program: recipe.map(|r| r.id.clone()),
                ..Default::default()
            },
        });
    }

    fn folder(&mut self, path: &Path, files: usize, reason: String, exact: bool) {
        let key = format!("folder:{}", path.display());
        self.push(Proposal {
            key,
            location: "folder",
            title: format!("{} ({files} file di log)", path.display()),
            reason,
            exact,
            kind: self.category.kind,
            program: None,
            program_name: None,
            format: None,
            bytes: Some(
                sources::file::folder_matching(path, None)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|f| fs::metadata(f).ok())
                    .map(|m| m.len())
                    .sum(),
            ),
            preview: vec![],
            readable: true,
            problem: None,
            source: NewSource {
                kind: "folder".into(),
                path: Some(path.to_owned()),
                log_kind: self.category.kind,
                ..Default::default()
            },
        });
    }

    fn container(
        &mut self,
        c: &Listed,
        kind: LogKind,
        recipe: Option<&Recipe>,
        reason: String,
        exact: bool,
    ) {
        let key = format!("container:{}", c.id);
        if self.has(&key) && !exact {
            return;
        }
        let preview = if self.previews < PREVIEW_LIMIT {
            self.previews += 1;
            command_lines(c.runtime.command(None).args(["logs", "--tail", "3", &c.id]))
        } else {
            vec![]
        };
        let recipe = recipe.or_else(|| recipes::for_image(&c.image));
        self.push(Proposal {
            key,
            location: "container",
            title: format!(
                "container «{}» · {} · {}",
                c.name,
                c.runtime.name(),
                recipes::image_name(&c.image)
            ),
            reason,
            exact,
            kind,
            program: recipe.map(|r| r.id.clone()),
            program_name: None,
            format: sample_format(&preview),
            bytes: None,
            preview,
            readable: true,
            problem: None,
            source: NewSource {
                kind: match c.runtime {
                    sources::podman::Runtime::Podman => "podman",
                    sources::podman::Runtime::Docker => "docker",
                }
                .into(),
                container: Some(c.id.clone()),
                log_kind: kind,
                program: recipe.map(|r| r.id.clone()),
                ..Default::default()
            },
        });
    }

    fn journal(
        &mut self,
        filter: Filter,
        title: String,
        kind: LogKind,
        recipe: Option<&Recipe>,
        reason: String,
        exact: bool,
    ) {
        let key = format!(
            "journal:{}",
            serde_json::to_string(&filter).unwrap_or_default()
        );
        if self.has(&key) && !exact {
            return;
        }
        let preview = if self.previews < PREVIEW_LIMIT {
            self.previews += 1;
            let mut cmd = Command::new("journalctl");
            cmd.args(["-n", "3", "-o", "cat", "--no-pager"]);
            for u in &filter.units {
                cmd.args(["-u", u]);
            }
            for i in &filter.identifiers {
                cmd.args(["-t", i]);
            }
            if let Some(p) = filter.priority {
                cmd.arg(format!("--priority={p}"));
            }
            command_lines(&mut cmd)
        } else {
            vec![]
        };
        self.push(Proposal {
            key,
            location: "journal",
            title,
            reason,
            exact,
            kind,
            program: recipe.map(|r| r.id.clone()),
            program_name: None,
            format: None,
            bytes: None,
            preview,
            readable: true,
            problem: None,
            source: NewSource {
                kind: "journal".into(),
                journal: Some(filter),
                log_kind: kind,
                program: recipe.map(|r| r.id.clone()),
                ..Default::default()
            },
        });
    }
}

fn command_lines(cmd: &mut Command) -> Vec<String> {
    crate::process::output(cmd, Duration::from_secs(5))
        .map(|out| {
            String::from_utf8_lossy(&out)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.chars().take(200).collect())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(3)
        .rev()
        .collect()
}

/// File di log nelle cartelle abituali, senza scendere troppo e con un limite.
fn scan_log_files(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<(PathBuf, usize)> = SCAN_DIRS
        .iter()
        .map(PathBuf::from)
        .chain(extra.iter().cloned())
        .map(|d| (d, 0))
        .collect();
    let mut files = Vec::new();
    let mut visited = 0;
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > SCAN_LIMIT {
                return files;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() && depth < 4 {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with('.') && name != "node_modules" && name != "proc" {
                    dirs.push((path, depth + 1));
                }
            } else if kind.is_file()
                && sources::file::log_name(&path)
                && !path.to_string_lossy().ends_with(".gz")
            {
                files.push(path);
            }
        }
    }
    files
}

fn kind_from_name(path: &Path, default: LogKind) -> LogKind {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if name.contains("access") {
        LogKind::Access
    } else if name.contains("error") || name.ends_with(".err") {
        LogKind::Error
    } else {
        default
    }
}

/// Il passo 3: i log trovati, ognuno con il motivo, e cosa manca.
pub fn proposals(
    category_id: &str,
    name: &str,
    chosen: &[String],
    machine: &Machine,
) -> Result<Proposals> {
    let category = category(category_id)?;
    let mut b = Builder {
        out: Vec::new(),
        keys: HashSet::new(),
        previews: 0,
        category,
    };
    let name = name.trim();
    let variants = variants(name);
    // I programmi: quelli scelti al passo 2, oppure quelli trovati della categoria.
    let programs: Vec<&Recipe> = if !chosen.is_empty() {
        chosen.iter().filter_map(|id| recipes::get(id)).collect()
    } else if let Some(group) = category.recipes {
        recipes::all()
            .iter()
            .filter(|r| r.in_category(group) && !recipes::found(r, machine).is_empty())
            .collect()
    } else {
        vec![]
    };
    let journal_ok = !machine.units.is_empty();

    for recipe in &programs {
        let mut has_file = false;
        // a) La configurazione del programma.
        for log in recipes::config_logs(recipe) {
            let for_site = !name.is_empty() && log.hosts.iter().any(|h| host_matches(h, name));
            if !name.is_empty() && category.names_site && !log.hosts.is_empty() && !for_site {
                continue; // Il log di un altro sito.
            }
            let reason = if for_site {
                format!("è nella configurazione di {} per {name}", recipe.name)
            } else {
                format!("è nella configurazione di {}", recipe.name)
            };
            if log.path.exists() {
                has_file = true;
                b.file(
                    &log.path,
                    log.kind,
                    Some(recipe),
                    reason,
                    for_site || !category.names_site,
                );
            }
        }
        // b) I percorsi tipici del programma.
        for spec in &recipe.logs {
            for path in recipes::expand(&spec.path)
                .into_iter()
                .filter(|p| p.is_file())
            {
                has_file = true;
                b.file(
                    &path,
                    spec.kind,
                    Some(recipe),
                    format!("percorso tipico di {}", recipe.name),
                    !category.names_site,
                );
            }
        }
        // c) Il journal, se il programma scrive lì e non c'è già un file.
        if journal_ok {
            if let Some(spec) = &recipe.journal {
                let units: Vec<String> = spec
                    .units
                    .iter()
                    .filter(|u| machine.units.contains(*u))
                    .cloned()
                    .collect();
                let (filter, title) = if !units.is_empty() {
                    (
                        Filter {
                            units: units.clone(),
                            ..Default::default()
                        },
                        format!("journal · {}", units.join(", ")),
                    )
                } else if !spec.identifiers.is_empty()
                    && !recipes::found(recipe, machine).is_empty()
                {
                    (
                        Filter {
                            identifiers: spec.identifiers.clone(),
                            ..Default::default()
                        },
                        format!("journal · {}", spec.identifiers.join(", ")),
                    )
                } else {
                    (Filter::default(), String::new())
                };
                if !title.is_empty() {
                    let kind = recipe.logs.first().map_or(category.kind, |l| l.kind);
                    b.journal(
                        filter,
                        title,
                        kind,
                        Some(recipe),
                        format!("{} scrive nel journal", recipe.name),
                        !has_file && !category.names_site,
                    );
                }
            }
        }
        // d) I container con l'immagine del programma.
        for c in machine
            .containers
            .iter()
            .filter(|c| recipes::for_image(&c.image).is_some_and(|r| r.id == recipe.id))
        {
            let exact = variants
                .iter()
                .any(|v| c.name.eq_ignore_ascii_case(v) || c.project.eq_ignore_ascii_case(v));
            b.container(
                c,
                category.kind,
                Some(recipe),
                format!("container con l'immagine di {}", recipe.name),
                exact,
            );
        }
    }

    // Il nome del sito o del progetto in percorsi, container e servizi.
    if !variants.is_empty() {
        let files = scan_log_files(&[]);
        // Una cartella che porta il nome e contiene più log: si propone la cartella intera.
        let mut folders: Vec<(PathBuf, usize)> = Vec::new();
        for file in &files {
            if let Some(parent) = file.parent() {
                let dir_name = parent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_lowercase();
                if variants.contains(&dir_name) {
                    match folders.iter_mut().find(|(p, _)| p == parent) {
                        Some((_, n)) => *n += 1,
                        None => folders.push((parent.to_owned(), 1)),
                    }
                }
            }
        }
        folders.retain(|(_, n)| *n >= 2);
        for (dir, n) in &folders {
            b.folder(dir, *n, "la cartella ha il nome del sito".into(), true);
        }
        for file in files
            .iter()
            .filter(|f| !folders.iter().any(|(d, _)| f.parent() == Some(d.as_path())))
        {
            let text = file.to_string_lossy().to_lowercase();
            if !variants.iter().any(|v| text.contains(v.as_str())) {
                continue;
            }
            let exact = file.iter().any(|segment| {
                let s = segment.to_string_lossy().to_lowercase();
                variants.contains(&s)
                    || variants.iter().any(|v| {
                        s.strip_prefix(v.as_str())
                            .is_some_and(|rest| rest.starts_with(['.', '-', '_']))
                    })
            });
            let kind = kind_from_name(file, category.kind);
            b.file(
                file,
                kind,
                None,
                "il nome compare nel percorso".into(),
                exact,
            );
        }
        for c in &machine.containers {
            let exact = variants
                .iter()
                .any(|v| c.name.eq_ignore_ascii_case(v) || c.project.eq_ignore_ascii_case(v));
            let partial = variants.iter().any(|v| {
                c.name.to_lowercase().contains(v.as_str()) || c.project.eq_ignore_ascii_case(v)
            });
            if exact || partial {
                let reason = if !c.project.is_empty()
                    && variants.iter().any(|v| c.project.eq_ignore_ascii_case(v))
                {
                    format!("fa parte del progetto «{}»", c.project)
                } else {
                    format!("il container si chiama «{}»", c.name)
                };
                b.container(c, category.kind, None, reason, exact);
            }
        }
        if let Ok(pods) = sources::kubernetes::discover(None) {
            for pod in pods {
                if let Some(app) = pod
                    .app
                    .as_deref()
                    .filter(|a| variants.iter().any(|v| a.eq_ignore_ascii_case(v)))
                {
                    let target = sources::kubernetes::Target {
                        namespace: pod.namespace.clone(),
                        selector: Some(format!("app={app}")),
                        ..Default::default()
                    };
                    let key = format!("pod:{}:{app}", pod.namespace);
                    if b.has(&key) {
                        continue;
                    }
                    b.push(Proposal {
                        key,
                        location: "pod",
                        title: format!("pod dell'applicazione «{app}» · {}", pod.namespace),
                        reason: "l'etichetta app ha il nome del sito".into(),
                        exact: true,
                        kind: category.kind,
                        program: None,
                        program_name: None,
                        format: None,
                        bytes: None,
                        preview: vec![],
                        readable: true,
                        problem: None,
                        source: NewSource {
                            kind: "kubernetes".into(),
                            kubernetes: Some(target),
                            log_kind: category.kind,
                            ..Default::default()
                        },
                    });
                }
            }
        }
        for unit in &machine.units {
            let stem = unit.strip_suffix(".service").unwrap_or(unit).to_lowercase();
            let exact = variants.contains(&stem);
            if exact || variants.iter().any(|v| stem.contains(v.as_str())) {
                b.journal(
                    Filter {
                        units: vec![unit.clone()],
                        ..Default::default()
                    },
                    format!("journal · {unit}"),
                    category.kind,
                    recipes::for_journal(std::slice::from_ref(unit), &[]),
                    format!("il servizio si chiama «{unit}»"),
                    exact,
                );
            }
        }
    }

    // Il sistema: il journal (solo avvisi ed errori) o i file classici.
    if category.id == "system" {
        if journal_ok {
            b.journal(
                Filter {
                    priority: Some(4),
                    ..Default::default()
                },
                "journal · avvisi ed errori di tutto il sistema".into(),
                LogKind::System,
                None,
                "il registro di sistema".into(),
                true,
            );
            b.journal(
                Filter {
                    identifiers: vec!["kernel".into()],
                    ..Default::default()
                },
                "journal · kernel".into(),
                LogKind::System,
                None,
                "i messaggi del kernel".into(),
                false,
            );
        }
        for path in ["/var/log/syslog", "/var/log/messages", "/var/log/kern.log"] {
            let path = Path::new(path);
            if path.is_file() {
                b.file(
                    path,
                    LogKind::System,
                    None,
                    "registro di sistema classico".into(),
                    !journal_ok,
                );
            }
        }
    }

    // Cosa manca, per sito e servizio web.
    let mut warnings = Vec::new();
    if category.asks_program && category.recipes == Some("web") {
        let note = |key: &str| {
            programs
                .first()
                .and_then(|r| r.note(key).map(|n| format!(" Per {}: {n}", r.name)))
                .unwrap_or_default()
        };
        if !b.out.iter().any(|p| p.kind == LogKind::Access) {
            warnings.push(format!(
                "Non ho trovato il log degli accessi.{}",
                note("access")
            ));
        }
        if !b.out.iter().any(|p| p.kind == LogKind::Error) {
            warnings.push(format!(
                "Non ho trovato il log degli errori.{}",
                note("error")
            ));
        }
    }
    if b.out.is_empty() && category.id != "other" {
        warnings.push(
            "Non ho trovato niente da solo: aggiungi il log a mano con «Manca qualcosa?».".into(),
        );
    }
    for recipe in &programs {
        if let Some(note) = recipe.note("rotation") {
            warnings.push(format!("{}: {note}", recipe.name));
        }
    }
    // Prima le corrispondenze certe, poi quelle leggibili.
    b.out.sort_by_key(|p| (!p.exact, !p.readable));
    Ok(Proposals {
        proposals: b.out,
        warnings,
    })
}

/// Cosa fare con un log che non sta in un file: Loglem legge file, cartelle, container e
/// journal, quindi basta far scrivere il programma in uno di questi posti.
pub const OTHER_HELP: &str = "Loglem legge file, cartelle, container e il journal di sistema.\n\
Se un programma manda i log altrove (a un servizio esterno, a una porta di rete, solo a video),\n\
fallo scrivere in uno di questi posti:\n\
• in un file: di solito un'opzione come «log file» o «log path» nella sua configurazione;\n\
• nel journal: se gira come servizio systemd, quello che scrive a video finisce già nel journal.\n\
Poi torna qui e aggiungilo come file o come journal.";

/// Un log scelto a mano con «Manca qualcosa?».
pub fn manual(mut source: NewSource, title: String, kind: LogKind) -> Proposal {
    source.log_kind = kind;
    let location = match source.kind.as_str() {
        "folder" => "folder",
        "podman" | "docker" => "container",
        "kubernetes" => "pod",
        "journal" => "journal",
        _ => "file",
    };
    let preview = match (location, source.path.as_deref()) {
        ("file", Some(path)) => tail_lines(path, 3).unwrap_or_default(),
        _ => vec![],
    };
    Proposal {
        key: format!("manual:{title}"),
        location,
        format: sample_format(&preview),
        bytes: source
            .path
            .as_deref()
            .filter(|p| p.is_file())
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len()),
        title,
        reason: "scelto da te".into(),
        exact: true,
        kind,
        program: None,
        program_name: None,
        preview,
        readable: true,
        problem: None,
        source,
    }
}

// ---------- passo 4: riepilogo e salvataggio ----------

#[derive(Debug, Clone, Deserialize)]
pub struct SaveItem {
    pub source: NewSource,
    #[serde(default)]
    pub kind: Option<LogKind>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveRequest {
    pub category: String,
    #[serde(default)]
    pub name: String,
    pub items: Vec<SaveItem>,
    #[serde(default)]
    pub retention: Option<Retention>,
    /// Aggiunti dalla pagina di un sito: i log vanno in quel sito, qualunque sia la scelta.
    #[serde(default)]
    pub site: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub site: String,
    pub sources: Vec<String>,
}

pub fn kind_word(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Access => "accessi",
        LogKind::Error => "errori",
        LogKind::Application => "app",
        LogKind::Database => "database",
        LogKind::AdminAccess => "accessi-admin",
        LogKind::System => "sistema",
        LogKind::Other => "log",
    }
}

pub fn kind_label(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Access => "Accessi",
        LogKind::Error => "Errori",
        LogKind::Application => "Applicazione",
        LogKind::Database => "Database",
        LogKind::AdminAccess => "Accessi di amministratori",
        LogKind::System => "Sistema",
        LogKind::Other => "Altro",
    }
}

fn slug(text: &str) -> String {
    let s: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    s.chars().take(40).collect()
}

/// Un identificatore libero e leggibile: `example-com-accessi`, `example-com-accessi-2`…
pub fn free_id(taken: &HashSet<String>, base: &str, kind: LogKind) -> String {
    let base = if base.is_empty() {
        "log".to_owned()
    } else {
        base.to_owned()
    };
    let first = format!("{base}-{}", kind_word(kind));
    if !taken.contains(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{first}-{n}"))
        .find(|id| !taken.contains(id))
        .unwrap_or(first)
}

/// Salva tutto quello scelto: crea il sito se serve e una sorgente per ogni log.
pub fn save(dir: &Path, request: SaveRequest) -> Result<Saved> {
    let category = category(&request.category)?;
    ensure!(!request.items.is_empty(), "Scegli almeno un log");
    let name = request.name.trim();
    if category.names_site {
        ensure!(!name.is_empty(), "Indica il nome del sito o del progetto");
    }
    let cfg = config::load(dir)?;
    let site = match request
        .site
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => {
            ensure!(cfg.sites.contains_key(id), "Sito sconosciuto: {id}");
            id.to_owned()
        }
        None if category.names_site => name.to_owned(),
        None => SERVER_SITE.to_owned(),
    };
    let mut taken: HashSet<String> = cfg.sources.iter().map(|s| s.id.clone()).collect();
    let base = if !name.is_empty() {
        slug(name)
    } else {
        request
            .items
            .first()
            .and_then(|i| i.source.program.clone())
            .unwrap_or_else(|| category.id.to_owned())
    };
    let display = if name.is_empty() {
        category.title.to_owned()
    } else {
        name.to_owned()
    };
    let mut created = Vec::new();
    let mut site_id = String::new();
    for item in request.items {
        let mut source = item.source;
        let kind = item.kind.unwrap_or(source.log_kind);
        let id = free_id(&taken, &base, kind);
        if source.kind == "journal"
            && source
                .journal
                .as_ref()
                .is_some_and(|f| *f == Filter::default())
        {
            bail!("Il journal intero senza filtri non si aggiunge da qui: scegli servizi, programmi o gravità");
        }
        source.name = id.clone();
        source.site = site.clone();
        source.log_kind = kind;
        source.label = Some(format!("{display} · {}", kind_label(kind)));
        if let Some(retention) = request.retention {
            source.retention = Some(retention);
        }
        site_id = crate::management::create(dir, source)?;
        taken.insert(id.clone());
        created.push(id);
    }
    Ok(Saved {
        site: site_id,
        sources: created,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_two_appears_only_where_it_helps_find_the_logs() {
        let asks: Vec<_> = CATEGORIES
            .iter()
            .filter(|c| c.asks_program)
            .map(|c| c.id)
            .collect();
        assert_eq!(asks, ["web", "service", "database", "mail"]);
        for c in CATEGORIES {
            if let Some(group) = c.recipes {
                assert!(
                    recipes::all().iter().any(|r| r.in_category(group)),
                    "{}",
                    c.id
                );
            }
            // Solo sito, servizio e applicazione diventano un sito.
            assert_eq!(
                c.names_site,
                ["web", "service", "app"].contains(&c.id),
                "{}",
                c.id
            );
        }
    }

    #[test]
    fn name_variants() {
        let v = variants("www.Example.com");
        assert!(v.contains(&"example.com".to_owned()));
        assert!(v.contains(&"example-com".to_owned()));
        assert!(v.contains(&"example_com".to_owned()));
        assert!(v.contains(&"example".to_owned()));
        // Le parole generiche non diventano varianti: «api» troverebbe tutto.
        assert!(!variants("api.example.com").contains(&"api".to_owned()));
        assert!(variants("ab").is_empty());
        assert!(host_matches("www.example.com", "example.com"));
        assert!(host_matches("*.example.com", "shop.example.com"));
        assert!(!host_matches("other.com", "example.com"));
    }

    fn machine(processes: &[&str]) -> Machine {
        Machine {
            processes: processes.iter().map(|s| s.to_string()).collect(),
            containers: vec![],
            units: HashSet::new(),
        }
    }

    #[test]
    fn found_programs_come_first_and_one_with_the_site_is_proposed() {
        let m = machine(&["nginx"]);
        let list = programs("web", "", &m).unwrap();
        assert_eq!(list.programs[0].id, "nginx");
        assert!(!list.programs[0].found.is_empty());
        // In ordine alfabetico dopo quelli trovati.
        let rest: Vec<_> = list.programs[1..]
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        let mut sorted = rest.clone();
        sorted.sort();
        assert_eq!(rest, sorted);
        // Senza il sito nella configurazione nessuna proposta già fatta.
        assert!(list.suggested.is_none());
        // Il passo 2 non ha programmi per «un altro log».
        assert!(programs("other", "", &m).unwrap().programs.is_empty());
    }

    #[test]
    fn i_dont_know_still_reaches_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let file = logs.path().join("shop-access.log");
        fs::write(
            &file,
            "203.0.113.4 - - [17/Nov/2026:10:00:00 +0000] \"GET / HTTP/1.1\" 200 5\n",
        )
        .unwrap();
        // «Non lo so» al passo 2: nessun programma scelto, il log si aggiunge a mano.
        let found = proposals("web", "shop.example", &[], &machine(&[])).unwrap();
        assert!(found.warnings.iter().any(|w| w.contains("accessi")));
        let saved = save(
            dir.path(),
            SaveRequest {
                category: "web".into(),
                name: "shop.example".into(),
                items: vec![SaveItem {
                    source: NewSource {
                        kind: "file".into(),
                        path: Some(file.clone()),
                        ..Default::default()
                    },
                    kind: Some(LogKind::Access),
                }],
                retention: None,
                site: None,
            },
        )
        .unwrap();
        assert_eq!(saved.sources, vec!["shop-example-accessi".to_owned()]);
        let cfg = config::load(dir.path()).unwrap();
        let source = &cfg.sources[0];
        assert_eq!(cfg.sites[&saved.site].name, "shop.example");
        assert_eq!(source.log_kind, LogKind::Access);
        assert_eq!(source.label.as_deref(), Some("shop.example · Accessi"));
        // Un altro salvataggio non riusa lo stesso nome e non crea un secondo sito.
        let again = save(
            dir.path(),
            SaveRequest {
                category: "web".into(),
                name: "shop.example".into(),
                items: vec![SaveItem {
                    source: NewSource {
                        kind: "file".into(),
                        path: Some(file),
                        ..Default::default()
                    },
                    kind: Some(LogKind::Access),
                }],
                retention: None,
                site: None,
            },
        )
        .unwrap();
        assert_eq!(again.sources, vec!["shop-example-accessi-2".to_owned()]);
        assert_eq!(again.site, saved.site);
    }

    #[test]
    fn logs_that_are_not_sites_go_to_this_server() {
        let dir = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let file = logs.path().join("mail.log");
        fs::write(&file, "Nov 18 10:00:00 host postfix/smtpd[1]: connect\n").unwrap();
        let saved = save(
            dir.path(),
            SaveRequest {
                category: "mail".into(),
                name: String::new(),
                items: vec![SaveItem {
                    source: NewSource {
                        kind: "file".into(),
                        path: Some(file),
                        program: Some("postfix".into()),
                        ..Default::default()
                    },
                    kind: Some(LogKind::Other),
                }],
                retention: None,
                site: None,
            },
        )
        .unwrap();
        assert_eq!(saved.site, SERVER_SITE);
        assert_eq!(saved.sources, vec!["postfix-log".to_owned()]);
    }

    #[test]
    fn proposals_find_files_by_name_and_explain_unreadable_ones() {
        // La ricerca per nome guarda le cartelle abituali: qui si prova la parte che
        // decide «certo / possibile» e il motivo, con un percorso costruito a mano.
        let logs = tempfile::tempdir().unwrap();
        let site_dir = logs.path().join("shop.example");
        fs::create_dir(&site_dir).unwrap();
        let access = site_dir.join("access.log");
        fs::write(&access, "{\"status\":200,\"uri\":\"/\"}\n").unwrap();
        let mut b = Builder {
            out: vec![],
            keys: HashSet::new(),
            previews: 0,
            category: category("web").unwrap(),
        };
        b.file(
            &access,
            LogKind::Access,
            None,
            "il nome compare nel percorso".into(),
            true,
        );
        let p = &b.out[0];
        assert!(p.exact && p.readable);
        assert_eq!(p.format.as_deref(), Some("JSON"));
        assert_eq!(p.preview.len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = site_dir.join("error.log");
            fs::write(&locked, "x\n").unwrap();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
            // Da root si legge comunque: la prova ha senso solo da utente normale.
            if fs::File::open(&locked).is_err() {
                b.file(&locked, LogKind::Error, None, "prova".into(), true);
                let q = b
                    .out
                    .iter()
                    .find(|p| p.title.ends_with("error.log"))
                    .unwrap();
                assert!(!q.readable && !q.exact);
                assert!(q.problem.as_deref().unwrap().contains("usermod"));
            }
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    #[test]
    fn ids_are_readable_and_unique() {
        let mut taken = HashSet::new();
        assert_eq!(
            free_id(&taken, "example-com", LogKind::Access),
            "example-com-accessi"
        );
        taken.insert("example-com-accessi".into());
        assert_eq!(
            free_id(&taken, "example-com", LogKind::Access),
            "example-com-accessi-2"
        );
        assert_eq!(slug("Shop.Example"), "shop-example");
        assert!(
            config::validate_id(&free_id(&taken, &slug("Negozio online!"), LogKind::Error)).is_ok()
        );
    }
}
