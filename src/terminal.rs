use crate::{
    cli::{self, Action, ConfigAction, ServiceAction, UpdateAction},
    collector, config,
    model::{Input, Retention, RetentionUnit, Source, SourceStatus},
    sources, timestamps, util,
};
use anyhow::{bail, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, ClearType},
};
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let screen = Self;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(screen)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn draw(title: &str, lines: &[String], footer: &str) -> Result<()> {
    let (width, height) = terminal::size()?;
    let mut out = io::stdout();
    execute!(out, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All))?;
    writeln!(
        out,
        "Loglem {} — {}\r",
        crate::VERSION,
        util::safe_text(title)
    )?;
    writeln!(out, "\r")?;
    for line in lines.iter().take(height.saturating_sub(4) as usize) {
        let safe = util::safe_text(line);
        writeln!(
            out,
            "{}\r",
            safe.chars()
                .take(width.saturating_sub(1) as usize)
                .collect::<String>()
        )?;
    }
    execute!(out, cursor::MoveTo(0, height.saturating_sub(1)))?;
    write!(
        out,
        "{}",
        footer
            .chars()
            .take(width.saturating_sub(1) as usize)
            .collect::<String>()
    )?;
    out.flush()?;
    Ok(())
}

pub fn menu(title: &str, choices: &[String]) -> Result<Option<usize>> {
    guided_menu(title, "", choices)
}

fn guided_menu(title: &str, help: &str, choices: &[String]) -> Result<Option<usize>> {
    guided_menu_from(title, help, choices, 0)
}

/// Come `guided_menu`, ma con il cursore inizialmente sulla voce `start`.
fn guided_menu_from(
    title: &str,
    help: &str,
    choices: &[String],
    start: usize,
) -> Result<Option<usize>> {
    if choices.is_empty() {
        page(
            title,
            "Non ci sono elementi disponibili.\nAggiungi prima una sorgente dal menu principale.",
        )?;
        return Ok(None);
    }
    let _screen = Screen::enter()?;
    let mut selected = start.min(choices.len() - 1);
    loop {
        let (_, height) = terminal::size()?;
        let header = help
            .lines()
            .map(str::to_owned)
            .chain(std::iter::once(String::new()))
            .collect::<Vec<_>>();
        let visible = (height as usize).saturating_sub(4 + header.len()).max(1);
        let start = selected.saturating_sub(visible - 1);
        let mut lines = header;
        lines.extend(
            choices
                .iter()
                .enumerate()
                .skip(start)
                .map(|(i, s)| format!("{} {}", if i == selected { ">" } else { " " }, s)),
        );
        draw(
            title,
            &lines,
            "↑/↓: scegli   Invio: apri / conferma   Esc: annulla",
        )?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1).min(choices.len() - 1)
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = choices.len() - 1,
                KeyCode::Enter => return Ok(Some(selected)),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None)
                }
                _ => {}
            }
        }
    }
}

fn choose(title: &str, choices: &[&str]) -> Result<Option<usize>> {
    menu(
        title,
        &choices.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
}

pub fn page(title: &str, text: &str) -> Result<()> {
    let _screen = Screen::enter()?;
    let lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut offset = 0usize;
    loop {
        draw(
            title,
            &lines[offset..],
            "↑/↓: scorri   PgUp/PgDn: pagina   Invio/Esc: torna indietro",
        )?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            let step = terminal::size()?.1.saturating_sub(4).max(1) as usize;
            match key.code {
                KeyCode::Down => offset = (offset + 1).min(lines.len().saturating_sub(1)),
                KeyCode::Up => offset = offset.saturating_sub(1),
                KeyCode::PageDown => offset = (offset + step).min(lines.len().saturating_sub(1)),
                KeyCode::PageUp => offset = offset.saturating_sub(step),
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(())
                }
                _ => {}
            }
        }
    }
}

/// Keep text fields inside the same screen; invalid input can be corrected in place.
fn field(
    title: &str,
    help: &str,
    default: &str,
    validate: impl Fn(&str) -> std::result::Result<(), String>,
) -> Result<Option<String>> {
    let _screen = Screen::enter()?;
    let mut value = default.to_owned();
    let mut error = String::new();
    loop {
        let (_, height) = terminal::size()?;
        let mut lines = help
            .lines()
            .take(height.saturating_sub(9) as usize)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        lines.push(String::new());
        let width = terminal::size()?.0.saturating_sub(5) as usize;
        let shown = value
            .chars()
            .rev()
            .take(width)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        lines.push(format!("> {shown}_"));
        lines.push(String::new());
        lines.push(error.clone());
        draw(
            title,
            &lines,
            "Scrivi il valore   Ctrl+U: svuota   Invio: continua   Esc: annulla",
        )?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None)
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    value.clear();
                    error.clear();
                }
                KeyCode::Enter => match validate(value.trim()) {
                    Ok(()) => return Ok(Some(value.trim().to_owned())),
                    Err(message) => error = message,
                },
                KeyCode::Backspace => {
                    value.pop();
                    error.clear();
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && value.len() < 4096 =>
                {
                    value.push(c);
                    error.clear();
                }
                _ => {}
            }
        }
    }
}

fn prompt(label: &str, default: &str) -> Result<String> {
    field(label, "", default, |_| Ok(()))?
        .ok_or_else(|| anyhow::anyhow!("Operazione annullata; nessuna modifica salvata"))
}

fn pick_period(title: &str, current: Retention) -> Result<Option<Retention>> {
    let options = vec![
        format!("{current} (valore attuale)"),
        "7 giorni".into(),
        "14 giorni".into(),
        "28 giorni".into(),
        "90 giorni".into(),
        "Altra durata: giorni, mesi o anni…".into(),
    ];
    let help = "Le copie scadute vengono eliminate automaticamente durante la raccolta.\n\
                Mesi e anni seguono il calendario. I file originali hanno una gestione separata.";
    match guided_menu(title, help, &options)? {
        None => Ok(None),
        Some(0) => Ok(Some(current)),
        Some(n @ 1..=4) => Ok(Some(Retention {
            amount: [7, 14, 28, 90][n - 1],
            unit: RetentionUnit::Days,
        })),
        _ => custom_period(current),
    }
}

/// Durata libera: prima l'unità (giorni, mesi, anni), poi il numero.
fn custom_period(current: Retention) -> Result<Option<Retention>> {
    let Some(unit) = choose("Unità di conservazione", &["Giorni", "Mesi", "Anni"])? else {
        return Ok(None);
    };
    let unit = [
        RetentionUnit::Days,
        RetentionUnit::Months,
        RetentionUnit::Years,
    ][unit];
    let validate = |value: &str| -> std::result::Result<(), String> {
        let amount = value
            .parse::<u32>()
            .map_err(|_| "Scrivi un numero intero".to_owned())?;
        Retention { amount, unit }
            .validate()
            .map_err(|e| e.to_string())
    };
    let Some(amount) = field(
        "Quanto tempo vuoi conservare i log?",
        "Inserisci un numero. Esempi: 28 giorni, 6 mesi, 2 anni.",
        &current.amount.to_string(),
        validate,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(Retention {
        amount: amount.parse()?,
        unit,
    }))
}

#[derive(Debug)]
struct FolderContents {
    folders: Vec<PathBuf>,
    logs: Vec<PathBuf>,
}

fn folder_contents(path: &Path) -> Result<FolderContents> {
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            folders.push(entry.path());
        }
    }
    folders.sort();
    Ok(FolderContents {
        folders,
        logs: sources::file::folder_files(path)?,
    })
}

/// Browse directories, preview their .log files, or enter an absolute path.
fn browse(folder_only: bool) -> Result<Option<PathBuf>> {
    let mut path = std::env::current_dir()?;
    loop {
        let contents = match folder_contents(&path) {
            Ok(contents) => contents,
            Err(error) => {
                page(
                    "Cartella non leggibile",
                    &format!(
                        concat!(
                            "{}\n",
                            "\n",
                            "Non riesco ad aprire questa cartella: {error}\n",
                            "Scegli una cartella a cui il tuo utente può accedere."
                        ),
                        path.display(),
                        error = error
                    ),
                )?;
                path = path.parent().unwrap_or(Path::new("/")).to_owned();
                continue;
            }
        };
        let help = format!(
            "Cartella: {}\n{} file .log trovati. Apri una sottocartella o scrivi un percorso.\n{}",
            path.display(),
            contents.logs.len(),
            if folder_only {
                "Scegli questa cartella per seguire anche i nuovi .log. Sottocartelle escluse."
            } else {
                "Seleziona il file .log da leggere."
            }
        );
        let mut choices = vec![
            if folder_only {
                "Usa questa cartella".into()
            } else {
                "Aggiorna elenco".into()
            },
            "Scrivi / incolla un percorso…".into(),
            "..  Cartella superiore".into(),
        ];
        choices.extend(contents.folders.iter().map(|p| {
            format!(
                "[Cartella] {}",
                p.file_name().unwrap_or_default().to_string_lossy()
            )
        }));
        choices.extend(contents.logs.iter().map(|p| {
            format!(
                "[.log] {}",
                p.file_name().unwrap_or_default().to_string_lossy()
            )
        }));
        let Some(index) = guided_menu("Aggiungi sorgente · 1/4 · Posizione", &help, &choices)?
        else {
            return Ok(None);
        };
        match index {
            0 if folder_only => return Ok(Some(path)),
            0 => {}
            1 => {
                let Some(entered) = field(
                    "Apri un percorso",
                    concat!(
                        "Percorso sul computer dove gira Loglem.\n",
                        "Esempi: /var/log oppure ~/Desktop/log\n",
                        "Puoi incollare il percorso; Invio per aprirlo."
                    ),
                    "",
                    |value| {
                        let expanded = expand_path(value);
                        if expanded.is_dir()
                            || (!folder_only
                                && expanded.is_file()
                                && expanded.extension().and_then(|v| v.to_str()) == Some("log"))
                        {
                            Ok(())
                        } else {
                            Err("Percorso non trovato. Indica una cartella leggibile o un file .log.".into())
                        }
                    },
                )?
                else {
                    continue;
                };
                let entered = std::fs::canonicalize(expand_path(&entered))?;
                if entered.is_file() {
                    return Ok(Some(entered));
                }
                path = entered;
            }
            2 => path = path.parent().unwrap_or(Path::new("/")).to_owned(),
            n if n < 3 + contents.folders.len() => path = contents.folders[n - 3].clone(),
            n => {
                let file = &contents.logs[n - 3 - contents.folders.len()];
                if folder_only {
                    let text = format!(
                        "{}\n\nPer aggiungere tutti i .log torna all'elenco e scegli\n\
                         «Usa questa cartella».",
                        file.display()
                    );
                    page("File presente nella cartella", &text)?;
                } else {
                    return Ok(Some(file.clone()));
                }
            }
        }
    }
}

fn expand_path(value: &str) -> PathBuf {
    let value = value.trim_matches(['\"', '\'']);
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest);
    }
    PathBuf::from(value)
}

fn suggested_id(path: &Path, dir: &Path) -> Result<String> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let base = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect::<String>();
    let base = if base.trim_matches('-').is_empty() {
        "log".to_owned()
    } else {
        base
    };
    let config = config::load(dir)?;
    let mut candidate = base.clone();
    let mut n = 2;
    while config.sources.iter().any(|s| s.id == candidate) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    Ok(candidate)
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

pub fn status_text(rows: &[SourceStatus]) -> String {
    if rows.is_empty() {
        return concat!(
            "Non hai ancora aggiunto sorgenti.\n",
            "Dal menu principale scegli «Aggiungi cartella, file o container»."
        )
        .into();
    }
    rows.iter()
        .map(source_summary)
        .collect::<Vec<_>>()
        .join("\n")
}

fn source_summary(s: &SourceStatus) -> String {
    let state = if s.enabled {
        "raccolta attiva"
    } else {
        "in pausa"
    };
    let original_size = s
        .original_bytes
        .map(human_bytes)
        .unwrap_or_else(|| "non disponibile".into());
    format!(
        "{name} — {state}\n  Legge da: {origin}\n  Spazio dei file originali: {original_size}\n  Salva le copie in: {archive}\n  Spazio delle copie: {archive_size}\n  Righe salvate: {entries} | Conservazione in Loglem: {days}\n  Originali: {original_policy}\n  Primo log in Loglem: {earliest}\n  Ultima lettura riuscita: {last}\n  Dettaglio: {message}\n",
        name = util::safe_text(&s.label),
        origin = util::safe_text(&s.origin),
        archive = util::safe_text(&s.archive),
        archive_size = human_bytes(s.archive_bytes),
        entries = s.entries,
        days = s.retention,
        original_policy = util::safe_text(&crate::sources::retention::originals(&s.input,s.remove_rotated,s.completed_dir.is_some())),
        earliest = s.earliest.as_deref().unwrap_or("nessun log salvato"),
        last = s.last_collection.as_deref().unwrap_or("non ancora eseguita"),
        message = util::safe_text(&s.message),
    )
}

const MAIN_MENU: [&str; 9] = [
    "Le mie sorgenti — posizione, spazio e log salvati",
    "Aggiungi cartella, file o container",
    "Cerca nei log",
    "Conservazione — per quanti giorni tenere i log",
    "Leggi i log adesso",
    "Raccolta automatica (servizio)",
    "Impostazioni",
    "Aggiornamenti",
    "Esci",
];

fn main_menu_help(has_sources: bool, dir: &Path) -> String {
    let text = if has_sources {
        "Cosa vuoi fare? Muoviti con ↑/↓ e premi Invio.\n\
         Per vedere i log nuovi scegli «Leggi i log adesso»; per trovarli, «Cerca nei log»."
    } else {
        "Benvenuto! Per iniziare bastano tre passi:\n  \
         1. «Aggiungi cartella, file o container»: indica dove si trovano i log.\n  \
         2. «Leggi i log adesso»: Loglem ne salva una copia ricercabile.\n  \
         3. «Cerca nei log»: trova parole e date.\n\
         I file originali restano dove sono, salvo che tu attivi la loro pulizia."
    };
    let mut help = format!("{text}\nDati salvati in: {}", dir.display());
    if let Ok(problems) = crate::health::check(dir, crate::health::DEFAULT_MAX_MINUTES, true) {
        for problem in problems.iter().take(3) {
            help.push_str(&format!("\n⚠ {}", problem.text));
        }
        if problems.len() > 3 {
            help.push_str(&format!(
                "\n⚠ … e altri {} (loglem check)",
                problems.len() - 3
            ));
        }
    }
    help
}

pub fn run(dir: &Path) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Il menu richiede un terminale interattivo. Per i comandi automatici usa «loglem --help».");
    }
    loop {
        let has_sources = !config::load(dir)?.sources.is_empty();
        // Senza sorgenti il cursore parte già su «Aggiungi».
        let start = if has_sources { 0 } else { 1 };
        let selected = guided_menu_from(
            "Menu principale",
            &main_menu_help(has_sources, dir),
            &MAIN_MENU.map(String::from),
            start,
        )?;
        let result = match selected {
            Some(0) => {
                collector::statuses(dir).and_then(|s| page("Le mie sorgenti", &status_text(&s)))
            }
            Some(1) => add_source(dir),
            Some(2) => search(dir),
            Some(3) => source_settings(dir),
            Some(4) => collector::once(dir).and_then(|_| {
                page(
                    "Lettura completata",
                    "I log sono stati archiviati. Apri Le mie sorgenti o Cerca nei log.",
                )
            }),
            Some(5) => service_menu(dir),
            Some(6) => config_menu(dir),
            Some(7) => update_menu(dir),
            _ => return Ok(()),
        };
        if let Err(error) = result {
            crate::diagnostics::error("terminal", "menu_action", None, &error);
            page(
                "Operazione non completata",
                &format!("{error:#}\n\nDettagli salvati in diagnostics/loglem.log"),
            )?;
        }
    }
}

/// Chiede quali servizi e programmi leggere dal journal (vuoto = tutto).
fn journal_filter() -> Result<Option<(Input, String, String)>> {
    let list = |value: &str| -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let Some(units) = field(
        "Journal · servizi",
        "Quali servizi systemd vuoi leggere? Separali con una virgola.\n\
         Esempio: caddy.service, podman.service\n\
         Lascia vuoto per leggere tutto il journal.",
        "",
        |_| Ok(()),
    )?
    else {
        return Ok(None);
    };
    let Some(identifiers) = field(
        "Journal · programmi",
        "Vuoi anche voci di programmi precisi? Separali con una virgola.\n\
         Esempio: sshd, sudo (accessi e uso dei privilegi). Lascia vuoto per nessun filtro.",
        "",
        |_| Ok(()),
    )?
    else {
        return Ok(None);
    };
    let filter = crate::sources::journal::Filter {
        units: list(&units),
        identifiers: list(&identifiers),
        matches: Vec::new(),
        priority: None,
    };
    filter.validate()?;
    let suggestion = match filter.units.as_slice() {
        [one] => one.trim_end_matches(".service").to_owned(),
        _ => "journal".to_owned(),
    };
    let description = format!("Journal di systemd: {}", filter.describe());
    Ok(Some((Input::Journal(filter), description, suggestion)))
}

fn add_source(dir: &Path) -> Result<()> {
    let Some(kind) = guided_menu(
        "Aggiungi sorgente",
        "Da dove vuoi leggere i log?\nScegli una cartella per aggiungere tutti i suoi file .log.",
        &[
            "Una cartella di file .log".into(),
            "Un singolo file .log".into(),
            "Un container Podman".into(),
            "Il log di accesso di Caddy (gestisce anche i file ruotati)".into(),
            "Il journal di Linux (servizi, accessi SSH, sudo)".into(),
        ],
    )?
    else {
        return Ok(());
    };
    let caddy = kind == 3;
    let (input, description, suggestion) = if kind == 4 {
        let Some(found) = journal_filter()? else {
            return Ok(());
        };
        found
    } else if kind != 2 {
        let Some(path) = browse(kind == 0)? else {
            return Ok(());
        };
        let suggestion = suggested_id(&path, dir)?;
        let description = if kind == 0 {
            let files = sources::file::folder_files(&path)?;
            format!(
                concat!(
                    "Cartella: {}\n",
                    "{} file .log presenti; anche i nuovi .log saranno letti.\n",
                    "Le sottocartelle non vengono incluse."
                ),
                path.display(),
                files.len()
            )
        } else if caddy {
            format!(
                "Log di Caddy: {}\n\
                 Anche i file ruotati da Caddy vengono letti e archiviati;\n\
                 Loglem li elimina 24 ore dopo, solo se la copia è verificata.",
                path.display()
            )
        } else {
            format!("File: {}", path.display())
        };
        (
            if kind == 0 {
                Input::Folder { path }
            } else if caddy {
                Input::File {
                    rotated_pattern: Some(crate::management::caddy_rotation_pattern(&path)?),
                    path,
                }
            } else {
                Input::File {
                    path,
                    rotated_pattern: None,
                }
            },
            description,
            suggestion,
        )
    } else {
        let Some(connection) = field(
            "Podman · connessione",
            "Lascia vuoto per usare la connessione Podman predefinita.",
            "",
            |_| Ok(()),
        )?
        else {
            return Ok(());
        };
        let connection = if connection.is_empty() {
            None
        } else {
            Some(connection)
        };
        let containers = sources::podman::discover(connection.as_deref())?;
        if containers.is_empty() {
            return page(
                "Podman",
                "Non ci sono container disponibili in questa connessione.",
            );
        }
        let labels = containers
            .iter()
            .map(|c| {
                format!(
                    "{} · pod: {} · {}",
                    c.name,
                    if c.pod.is_empty() { "nessuno" } else { &c.pod },
                    c.state
                )
            })
            .collect::<Vec<_>>();
        let Some(index) = menu("Scegli il container", &labels)? else {
            return Ok(());
        };
        (
            Input::Podman {
                container: containers[index].id.clone(),
                connection,
            },
            format!("Container: {}", containers[index].name),
            suggested_id(Path::new(&containers[index].name), dir)?,
        )
    };
    let Some(name) = field(
        "Aggiungi sorgente · 2/4 · Nome",
        concat!(
            "Come vuoi chiamarla nell'elenco?\n",
            "Puoi mantenere il nome proposto. Usa lettere, numeri, - oppure _."
        ),
        &suggestion,
        |value| {
            config::validate_id(value)
                .map_err(|_| "Usa da 1 a 64 lettere, numeri, - oppure _.".to_owned())?;
            match config::load(dir) {
                Ok(c) if c.sources.iter().any(|s| s.id == value) => {
                    Err("Nome già utilizzato. Scegli un altro nome.".into())
                }
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        },
    )?
    else {
        return Ok(());
    };
    let Some(days) = pick_period(
        "Aggiungi sorgente · 3/4 · Durata",
        Retention {
            amount: 28,
            unit: RetentionUnit::Days,
        },
    )?
    else {
        return Ok(());
    };
    let help = format!(
        concat!(
            "{description}\n",
            "Nome: {name}\n",
            "Copie conservate per: {days}\n",
            "Archivio: {}\n",
            "{originals}"
        ),
        config::archive_path(dir, &name).display(),
        originals = if caddy {
            "I file ruotati verranno eliminati dopo la verifica; il file attivo mai."
        } else {
            "I file originali non verranno cancellati."
        },
        description = description,
        name = name,
        days = days
    );
    let Some(choice) = guided_menu(
        "Aggiungi sorgente · 4/4 · Riepilogo",
        &help,
        &[
            "Salva e leggi i log adesso".into(),
            "Salva per leggere in seguito".into(),
            "Annulla".into(),
        ],
    )?
    else {
        return Ok(());
    };
    if choice == 2 {
        return Ok(());
    }
    config::add(
        dir,
        Source {
            id: name.clone(),
            retention: Some(days),
            retention_days: if days.unit == RetentionUnit::Days {
                days.amount
            } else {
                28
            },
            enabled: true,
            remove_rotated: caddy,
            log_kind: if caddy {
                crate::model::LogKind::Access
            } else {
                crate::model::LogKind::Generic
            },
            input: input.clone(),
            ..Source::default()
        },
    )?;
    let mut message = format!("Sorgente «{name}» aggiunta.\nConservazione: {days}.\n\n");
    if let (true, Input::File { path, .. }) = (caddy, &input) {
        message.push_str(&format!(
            "Nel Caddyfile usa questo blocco log, poi riavvia Caddy:\n\n{}\n\n",
            crate::management::caddy_snippet(path)
        ));
    }
    if choice == 0 {
        match collector::once(dir) {
            Ok(()) => message.push_str(
                "Prima lettura completata. Apri «Le mie sorgenti» per vedere i log archiviati.",
            ),
            Err(error) => message.push_str(&format!(
                concat!(
                    "La sorgente è salvata, ma la lettura non è stata completata.\n",
                    "Il raccoglitore potrebbe essere già attivo.\n",
                    "Dettaglio: {error:#}"
                ),
                error = error
            )),
        }
    } else {
        message.push_str("Quando vuoi, scegli «Leggi i log adesso» dal menu principale.");
    }
    message.push_str(concat!(
        "\n",
        "\n",
        "Per la raccolta continua usa «Raccolta automatica» (Linux).\n",
        "I menu da soli non avviano la raccolta continua."
    ));
    page("Sorgente salvata", &message)
}

fn search(dir: &Path) -> Result<()> {
    let sources = config::load(dir)?.sources;
    if sources.is_empty() {
        return page("Cerca nei log", "Non ci sono ancora sorgenti.\nScegli «Aggiungi cartella, file o container» dal menu principale.");
    }
    let mut choices = vec!["Tutte le sorgenti".into()];
    choices.extend(sources.iter().map(|s| s.id.clone()));
    let Some(index) = menu("Dove vuoi cercare?", &choices)? else {
        return Ok(());
    };
    let source = if index == 0 {
        None
    } else {
        Some(sources[index - 1].id.as_str())
    };
    let Some(text) = field(
        "Che cosa vuoi cercare?",
        "Scrivi una parola o un percorso. Lascia vuoto per mostrare tutti i log.",
        "",
        |_| Ok(()),
    )?
    else {
        return Ok(());
    };
    let Some(from) = field(
        "Da quale data?",
        "Esempio: 2026-11-16. Lascia vuoto per cercare dall'inizio.\nLe date senza ora sono in UTC.",
        "",
        |v| {
            if v.is_empty() {
                Ok(())
            } else {
                timestamps::parse_date(v, false)
                    .map(|_| ())
                    .map_err(|_| "Scrivi una data come 2026-11-16.".into())
            }
        },
    )?
    else {
        return Ok(());
    };
    let Some(to) = field(
        "Fino a quale data?",
        "Esempio: 2026-11-16, inclusa tutta la giornata. Lascia vuoto per oggi e oltre.",
        "",
        |v| {
            if v.is_empty() {
                Ok(())
            } else {
                timestamps::parse_date(v, true)
                    .map(|_| ())
                    .map_err(|_| "Scrivi una data come 2026-11-16.".into())
            }
        },
    )?
    else {
        return Ok(());
    };
    let rows = collector::search(
        dir,
        source,
        &crate::query::Query {
            text,
            ignore_case: true,
            ..Default::default()
        },
        if from.is_empty() {
            i64::MIN
        } else {
            timestamps::parse_date(&from, false)?
        },
        if to.is_empty() {
            i64::MAX
        } else {
            timestamps::parse_date(&to, true)?
        },
        200,
    )?;
    let text = if rows.is_empty() {
        "Nessun risultato.\nHai già eseguito una lettura con «Leggi i log adesso»?".into()
    } else {
        rows.iter()
            .map(|r| {
                format!(
                    "{} [{} · {}]\n{}\n",
                    r.timestamp,
                    r.source,
                    if r.timestamp_origin == "event" {
                        "data del log"
                    } else {
                        "data di lettura: data originale sconosciuta"
                    },
                    r.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    page("Risultati · fino a 200 righe più recenti", &text)
}

fn source_settings(dir: &Path) -> Result<()> {
    loop {
        let sources = config::load(dir)?.sources;
        if sources.is_empty() {
            if choose(
                "Conservazione dei log",
                &[
                    "Non hai ancora sorgenti: aggiungine una adesso",
                    "Torna al menu",
                ],
            )? == Some(0)
            {
                add_source(dir)?;
            } else {
                return Ok(());
            }
            continue;
        }
        let labels = sources
            .iter()
            .map(|s| {
                format!(
                    "{} · {} · {}",
                    s.label.as_deref().unwrap_or(&s.id),
                    s.period(),
                    if s.enabled {
                        "raccolta abilitata"
                    } else {
                        "in pausa"
                    }
                )
            })
            .collect::<Vec<_>>();
        let Some(index) = guided_menu(
            "Conservazione · Scegli la sorgente",
            concat!(
                "Qui decidi per quanti giorni conservare le copie di Loglem.\n",
                "Seleziona una sorgente per cambiare durata o mettere in pausa la raccolta."
            ),
            &labels,
        )?
        else {
            return Ok(());
        };
        let source = &sources[index];
        let options = vec![
            format!("Cambia durata: ora {}", source.period()),
            if source.enabled {
                "Metti in pausa la raccolta".into()
            } else {
                "Riprendi la raccolta".into()
            },
            "Cambia nome visualizzato".into(),
            "Cartella del database SQLite".into(),
            "Tipo di log".into(),
            "Pulizia dei file originali conclusi".into(),
            "Scollega mantenendo lo storico".into(),
            "Ricollega un container sostitutivo".into(),
            "Indietro".into(),
        ];
        match menu(&source.id, &options)? {
            Some(0) => {
                let Some(days) =
                    pick_period("Per quanto tempo vuoi conservare i log?", source.period())?
                else {
                    continue;
                };
                let explanation = format!(
                    concat!(
                        "Sorgente: {}\n",
                        "Durata precedente: {}\n",
                        "Nuova durata: {days}\n",
                        "\n",
                        "Alla prossima raccolta, le copie più vecchie della nuova durata\n",
                        "verranno eliminate dall'archivio. Gli originali non vengono toccati."
                    ),
                    source.id,
                    source.period(),
                    days = days
                );
                if guided_menu(
                    "Conferma conservazione",
                    &explanation,
                    &["Salva questa durata".into(), "Annulla".into()],
                )? == Some(0)
                {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name: source.id.clone(),
                            retention: Some(days),
                            ..Default::default()
                        },
                    )?;
                    let text = format!(
                        "{}: {days}.\nLa nuova durata è stata salvata. \
                         Verrà applicata alla prossima raccolta.",
                        source.id
                    );
                    page("Conservazione salvata", &text)?;
                }
            }
            Some(1) => {
                crate::management::apply(
                    dir,
                    crate::management::Change {
                        name: source.id.clone(),
                        enabled: Some(!source.enabled || source.detached),
                        ..Default::default()
                    },
                )?;
                page(
                    "Impostazione salvata",
                    concat!(
                        "La pausa ferma la lettura di nuove righe.\n",
                        "La scadenza delle copie continua a essere applicata dal raccoglitore."
                    ),
                )?;
            }
            Some(2) => {
                let Some(label) = field(
                    "Nome visualizzato",
                    "Puoi usare anche spazi. Lo storico rimane invariato.",
                    source.label.as_deref().unwrap_or(&source.id),
                    |s| {
                        if s.is_empty() {
                            Err("Inserisci un nome".into())
                        } else {
                            Ok(())
                        }
                    },
                )?
                else {
                    continue;
                };
                crate::management::apply(
                    dir,
                    crate::management::Change {
                        name: source.id.clone(),
                        label: Some(label),
                        ..Default::default()
                    },
                )?;
                page("Nome salvato", "Il nome visualizzato è stato aggiornato.")?;
            }
            Some(3) => {
                let default = dir.join("archives");
                let help = format!(
                    "Vuoto = cartella di Loglem: {}\n\
                     Percorso assoluto = cartella personalizzata.\n\
                     L'archivio esistente verrà copiato, verificato e spostato.",
                    default.display()
                );
                let current = source
                    .archive_dir
                    .as_ref()
                    .and_then(|p| p.to_str())
                    .unwrap_or("");
                let Some(path) = field("Dove salvare SQLite?", &help, current, |_| Ok(()))? else {
                    continue;
                };
                crate::management::apply(
                    dir,
                    crate::management::Change {
                        name: source.id.clone(),
                        archive_dir: Some(path),
                        ..Default::default()
                    },
                )?;
                page(
                    "Archivio aggiornato",
                    "La posizione del database è stata salvata. Lo storico resta consultabile.",
                )?;
            }
            Some(4) => {
                let Some(kind) = choose(
                    "Che tipo di log è?",
                    &[
                        "Accessi (può contenere IP)",
                        "Generico / applicazione",
                        "Errori e stack trace",
                    ],
                )?
                else {
                    continue;
                };
                crate::management::apply(
                    dir,
                    crate::management::Change {
                        name: source.id.clone(),
                        log_kind: Some(
                            [
                                crate::model::LogKind::Access,
                                crate::model::LogKind::Generic,
                                crate::model::LogKind::Error,
                            ][kind],
                        ),
                        ..Default::default()
                    },
                )?;
            }
            Some(5) => {
                if matches!(source.input, Input::Podman { .. }) {
                    page(
                        "Originali Podman",
                        "Gli originali sono gestiti da Podman. Loglem non cancella il journal di sistema.",
                    )?;
                    continue;
                }
                match choose(
                    "File originali",
                    &[
                        "Mantieni gli originali (solo lettura)",
                        "Archivia ed elimina file consegnati come conclusi",
                    ],
                )? {
                    Some(0) => {
                        crate::management::apply(
                            dir,
                            crate::management::Change {
                                name: source.id.clone(),
                                completed_dir: Some(String::new()),
                                ..Default::default()
                            },
                        )?;
                    }
                    Some(1) => {
                        page(
                            "Cartella dei file conclusi",
                            "Il programma deve consegnare qui solo file .log chiusi, che non modificherà più.\n\
                             La cartella deve essere separata dai file attivi.\n\
                             Loglem copia e verifica ogni file, e lo elimina solo se nessuno\n\
                             lo modifica da almeno 24 ore.",
                        )?;
                        let Some(path) = browse(true)? else {
                            continue;
                        };
                        if choose(
                            "Confermi che il produttore non scrive più su questi file?",
                            &["Annulla", "Sì, solo file conclusi"],
                        )? == Some(1)
                        {
                            crate::management::apply(
                                dir,
                                crate::management::Change {
                                    name: source.id.clone(),
                                    completed_dir: Some(path.display().to_string()),
                                    handoff_confirmed: true,
                                    ..Default::default()
                                },
                            )?;
                        }
                    }
                    _ => {}
                }
            }
            Some(6) => {
                if choose(
                    "Scollegare questa sorgente?",
                    &[
                        "Annulla",
                        "Scollega: conserva lo storico fino alla scadenza",
                    ],
                )? == Some(1)
                {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name: source.id.clone(),
                            detached: Some(true),
                            ..Default::default()
                        },
                    )?;
                }
            }
            Some(7) => {
                let Input::Podman { connection, .. } = &source.input else {
                    page(
                        "Ricollega container",
                        "Questa opzione vale per sorgenti Podman.",
                    )?;
                    continue;
                };
                let containers = sources::podman::discover(connection.as_deref())?;
                let labels = containers
                    .iter()
                    .map(|c| format!("{} · {}", c.name, &c.id[..12]))
                    .collect::<Vec<_>>();
                let Some(selected) = menu(
                    "Scegli il sostituto. Lo storico rimane nella stessa sorgente.",
                    &labels,
                )?
                else {
                    continue;
                };
                crate::management::apply(
                    dir,
                    crate::management::Change {
                        name: source.id.clone(),
                        container: Some(containers[selected].id.clone()),
                        enabled: Some(true),
                        ..Default::default()
                    },
                )?;
            }
            _ => {}
        }
    }
}

fn service_menu(dir: &Path) -> Result<()> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return page(
            "Raccolta automatica",
            "Il servizio automatico è disponibile su Linux e macOS.\n\
             Qui puoi usare «Leggi i log adesso», oppure tenere aperto\n\
             «loglem web» in un terminale: raccoglie i log e mostra la pagina.",
        );
    }
    let Some(choice) = choose(
        "Raccolta automatica (servizio)",
        &[
            "Installa e avvia il servizio (consigliato)",
            "Avvia",
            "Ferma",
            "Mostra lo stato",
        ],
    )?
    else {
        return Ok(());
    };
    if choice == 3 {
        let text = if crate::service::is_active()? {
            format!(
                "In esecuzione: i log vengono letti in automatico.\n{}",
                web_address_text(dir)?
            )
        } else {
            "Fermo: i log vengono letti solo con «Leggi i log adesso».".to_owned()
        };
        return page("Raccolta automatica", &text);
    }
    let action = match choice {
        0 => ServiceAction::Install { start: true },
        1 => ServiceAction::Start,
        _ => ServiceAction::Stop,
    };
    cli::execute(dir, Action::Service { action })?;
    let text = format!(
        "Fatto. Il servizio raccoglie i log e mostra la pagina web.\n{}\n\n\
         Per farlo partire all'accensione anche senza login, un amministratore\n\
         deve abilitare il «lingering» dell'utente (vedi il README).",
        web_address_text(dir)?
    );
    page("Raccolta automatica", &text)
}

fn web_address_text(dir: &Path) -> Result<String> {
    Ok(match config::load(dir)?.web_port {
        Some(port) => format!("Pagina web: http://127.0.0.1:{port}"),
        None => "La pagina web avrà un indirizzo fisso dopo il primo avvio.".to_owned(),
    })
}

fn config_menu(dir: &Path) -> Result<()> {
    let Some(choice) = choose(
        "Impostazioni",
        &[
            "Mostra dove Loglem salva i dati",
            "Esporta la configurazione in un file",
            "Importa la configurazione da un file",
            "Ogni quanti secondi leggere i log",
        ],
    )?
    else {
        return Ok(());
    };
    if choice == 0 {
        return page("Cartella dei dati", &dir.display().to_string());
    }
    let action = match choice {
        1 => ConfigAction::Export {
            output: prompt("Dove salvare il file di configurazione? (percorso)", "")?.into(),
        },
        2 => ConfigAction::Import {
            file: prompt(
                "File da importare (sostituisce le impostazioni attuali)",
                "",
            )?
            .into(),
        },
        _ => ConfigAction::Poll {
            seconds: prompt("Secondi tra una lettura e la successiva", "5")?.parse()?,
        },
    };
    cli::execute(dir, Action::Config { action })
}

fn update_menu(dir: &Path) -> Result<()> {
    let Some(choice) = choose(
        "Aggiornamenti",
        &[
            "Controlla se c'è una nuova versione",
            "Installa l'aggiornamento",
            "Stato dell'ultimo aggiornamento",
            "Imposta da dove scaricare gli aggiornamenti",
        ],
    )?
    else {
        return Ok(());
    };
    match choice {
        0 => {
            let manifest = crate::update::check(dir)?;
            page(
                "Aggiornamenti",
                &format!(
                    "Versione installata: {}\nUltima versione firmata: {}",
                    crate::VERSION,
                    manifest.version
                ),
            )
        }
        1 => {
            cli::execute(
                dir,
                Action::Update {
                    action: UpdateAction::Install,
                },
            )?;
            page("Aggiornamento", concat!("L'aggiornamento è partito in modo indipendente: puoi chiudere questo ", "menu.\n", "Per il risultato apri «Stato dell'ultimo aggiornamento»."))
        }
        2 => page(
            "Stato dell'aggiornamento",
            &std::fs::read_to_string(dir.join("updates/status.txt"))
                .unwrap_or_else(|_| "Nessun aggiornamento richiesto finora.".into()),
        ),
        _ => cli::execute(
            dir,
            Action::Update {
                action: UpdateAction::Configure {
                    repository: prompt("Repository GitHub (formato proprietario/nome)", "")?,
                    public_key: prompt(
                        "Chiave pubblica Ed25519 di cui ti fidi (base64, ottenuta da una fonte indipendente)",
                        "",
                    )?,
                },
            },
        ),
    }
}
