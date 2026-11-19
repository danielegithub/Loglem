use crate::{
    cli::{self, Action, ConfigAction, ServiceAction, UpdateAction},
    collector, config,
    model::{Input, Retention, RetentionUnit, SourceStatus},
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
    /// Tutti i file: un log può avere qualsiasi nome.
    files: Vec<PathBuf>,
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
    let mut files: Vec<PathBuf> = std::fs::read_dir(path)?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    Ok(FolderContents {
        folders,
        logs: files
            .iter()
            .filter(|p| sources::file::log_name(p))
            .cloned()
            .collect(),
        files,
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
            "Cartella: {}\n{} file che sembrano log, {} file in tutto. Apri una sottocartella o scrivi un percorso.\n{}",
            path.display(),
            contents.logs.len(),
            contents.files.len(),
            if folder_only {
                "Scegli questa cartella: dopo scegli i nomi dei file da leggere. Sottocartelle escluse."
            } else {
                "Seleziona il file da leggere: va bene qualsiasi file di testo."
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
        let shown = if folder_only {
            &contents.logs
        } else {
            &contents.files
        };
        choices.extend(shown.iter().map(|p| {
            format!(
                "[{}] {}",
                if sources::file::log_name(p) {
                    "log"
                } else {
                    "file"
                },
                p.file_name().unwrap_or_default().to_string_lossy()
            )
        }));
        let Some(index) = guided_menu("Scegli dove", &help, &choices)? else {
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
                        if expanded.is_dir() || (!folder_only && expanded.is_file()) {
                            Ok(())
                        } else {
                            Err(
                                "Percorso non trovato. Indica una cartella leggibile o un file."
                                    .into(),
                            )
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
                let file = &shown[n - 3 - contents.folders.len()];
                if folder_only {
                    let text = format!(
                        "{}\n\nPer aggiungere i file della cartella torna all'elenco e scegli\n\
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
            "Dal menu principale scegli «I miei siti → Nuovo sito»."
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
    "I miei siti",
    "Aggiungi un log",
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
         1. «I miei siti → Nuovo sito»: crea un sito e collegagli un log.\n  \
         2. «Leggi i log adesso»: Loglem ne salva una copia ricercabile.\n  \
         3. «Cerca nei log»: trova parole e date.\n\
         I file originali restano dove sono, salvo che tu attivi la loro pulizia."
    };
    let mut help = format!("{text}\nDati salvati in: {}", dir.display());
    // Se nessun Loglem tiene il blocco del servizio, nessuno sta leggendo i log.
    let reading = crate::util::Lock::acquire(&dir.join("service.lock")).is_err();
    help.push_str(&format!(
        "\nRaccolta: {}",
        if reading {
            "attiva"
        } else {
            "ferma · scegli «Raccolta automatica» per avviarla e tenerla accesa"
        }
    ));
    help.push_str(&format!("\n{}", crate::monitor::line(dir)));
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
        let selected = guided_menu_from(
            "Menu principale",
            &main_menu_help(has_sources, dir),
            &MAIN_MENU.map(String::from),
            0,
        )?;
        let result = match selected {
            Some(0) => site_menu(dir),
            Some(1) => add_source(dir, None),
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
         Esempio: myapp.service, nginx.service\n\
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

fn site_menu(dir: &Path) -> Result<()> {
    let collector::SiteStatus {
        sites,
        sources: rows,
        ..
    } = collector::statuses_with_sites(dir)?;
    let mut choices: Vec<_> = sites
        .values()
        .map(|site| (site.id.as_str(), site.name.as_str()))
        .collect();
    choices.sort_by_key(|(id, name)| (*id == crate::model::SERVER_SITE, *name));
    if rows.iter().any(|s| s.site.is_empty()) {
        choices.push(("", "Senza sito"));
    }
    let mut labels = vec!["Nuovo sito".to_owned()];
    labels.extend(choices.iter().map(|(id, name)| {
        format!(
            "{name} · {} log",
            rows.iter().filter(|s| s.site == *id).count()
        )
    }));
    let Some(index) = menu("I miei siti", &labels)? else {
        return Ok(());
    };
    let (id, name) = if index == 0 {
        let Some(name) = field("Nome del sito o progetto", "", "", |value| {
            if value.is_empty() {
                Err("Indica un nome.".into())
            } else {
                Ok(())
            }
        })?
        else {
            return Ok(());
        };
        (config::add_site(dir, &name)?, name)
    } else {
        let (id, name) = choices[index - 1];
        (id.to_owned(), name.to_owned())
    };
    let logs: Vec<_> = rows.into_iter().filter(|s| s.site == id).collect();
    let mut labels = vec![
        "Aggiungi un log".into(),
        "Vedi copie e doppioni del sito".into(),
        "Cerca doppioni anche fuori da Loglem".into(),
    ];
    labels.extend(logs.iter().map(|s| s.label.clone()));
    let summary = if logs.is_empty() {
        "Nessun log collegato."
    } else {
        "Scegli un log per vedere origine, archivio e stato."
    };
    match guided_menu(&name, summary, &labels)? {
        Some(0) => add_source(dir, Some(&id))?,
        Some(choice @ (1 | 2)) => {
            let report =
                crate::copies::report(dir, if id.is_empty() { "~" } else { &id }, choice == 2)?;
            page("Copie e doppioni", &report.description())?;
        }
        Some(index) => page(&logs[index - 3].label, &source_summary(&logs[index - 3]))?,
        None => {}
    }
    Ok(())
}

/// Il tipo di log, scelto da un elenco.
fn pick_kind(current: crate::model::LogKind) -> Result<crate::model::LogKind> {
    use crate::model::LogKind;
    let kinds = [
        LogKind::Access,
        LogKind::Error,
        LogKind::Application,
        LogKind::Database,
        LogKind::AdminAccess,
        LogKind::System,
        LogKind::Other,
    ];
    let labels: Vec<String> = kinds
        .iter()
        .map(|k| {
            format!(
                "{}{}",
                crate::wizard::kind_label(*k),
                if *k == current { " (attuale)" } else { "" }
            )
        })
        .collect();
    Ok(menu("Che tipo di log è?", &labels)?
        .map(|i| kinds[i])
        .unwrap_or(current))
}

/// «Manca qualcosa? Aggiungi da…»: un log scelto a mano, per dove sta.
fn pick_location(kind: crate::model::LogKind) -> Result<Option<crate::wizard::Proposal>> {
    use crate::management::NewSource;
    let Some(choice) = guided_menu(
        "Aggiungi da…",
        "Dove sta il log? Conta dove sta, non quale programma lo scrive.",
        &[
            "Una cartella (tutti i file con un certo nome, anche quelli futuri)".into(),
            "Un file".into(),
            "Un container (Podman, Docker) o un pod di Kubernetes".into(),
            "Il journal di sistema (servizi, programmi, gravità)".into(),
            "Altro: un log che non sta in un file".into(),
        ],
    )?
    else {
        return Ok(None);
    };
    let manual =
        |source: NewSource, title: String| Some(crate::wizard::manual(source, title, kind));
    match choice {
        0 => {
            let Some(path) = browse(true)? else {
                return Ok(None);
            };
            let mut patterns: Vec<String> = sources::file::suggest_patterns(&path)
                .into_iter()
                .map(|(p, n)| format!("{p} · {n} file"))
                .collect();
            if patterns.is_empty() {
                patterns.push(format!(
                    "{} · nessun file per ora",
                    sources::file::DEFAULT_FOLDER_PATTERN
                ));
            }
            patterns.push("Scrivo io il modello…".into());
            let Some(i) = guided_menu(
                "Quali file della cartella?",
                "Scegli i nomi dei file da leggere: * sta per «qualsiasi testo».\nVengono letti anche i file futuri con lo stesso nome. Le sottocartelle no.",
                &patterns,
            )?
            else {
                return Ok(None);
            };
            let pattern = if i == patterns.len() - 1 {
                prompt("Modello dei nomi (es. *.log, *_log, app-*.txt)", "*.log")?
            } else {
                patterns[i]
                    .split(" · ")
                    .next()
                    .unwrap_or("*.log")
                    .to_owned()
            };
            let pattern = sources::file::checked_pattern(&pattern)?;
            let title = format!("{} ({pattern})", path.display());
            Ok(manual(
                NewSource {
                    kind: "folder".into(),
                    path: Some(path),
                    pattern: Some(pattern),
                    ..Default::default()
                },
                title,
            ))
        }
        1 => {
            let Some(path) = browse(false)? else {
                return Ok(None);
            };
            let title = path.display().to_string();
            Ok(manual(
                NewSource {
                    kind: "file".into(),
                    path: Some(path),
                    ..Default::default()
                },
                title,
            ))
        }
        2 => {
            let containers = sources::podman::list_all();
            let pods = sources::kubernetes::discover(None).unwrap_or_default();
            if containers.is_empty() && pods.is_empty() {
                page(
                    "Container",
                    "Non vedo container né pod: Podman, Docker e kubectl non rispondono a questo utente.",
                )?;
                return Ok(None);
            }
            let mut labels: Vec<String> = containers
                .iter()
                .map(|c| {
                    format!(
                        "{} · {} · {} · {}",
                        c.name,
                        c.runtime.name(),
                        crate::recipes::image_name(&c.image),
                        c.state
                    )
                })
                .collect();
            labels.extend(pods.iter().map(|p| match &p.app {
                Some(app) => format!("pod {} · Kubernetes · app={app} · {}", p.name, p.namespace),
                None => format!("pod {} · Kubernetes · {}", p.name, p.namespace),
            }));
            let Some(i) = menu("Quale container?", &labels)? else {
                return Ok(None);
            };
            if let Some(c) = containers.get(i) {
                return Ok(manual(
                    NewSource {
                        kind: match c.runtime {
                            sources::podman::Runtime::Podman => "podman",
                            sources::podman::Runtime::Docker => "docker",
                        }
                        .into(),
                        container: Some(c.id.clone()),
                        ..Default::default()
                    },
                    format!("container «{}» · {}", c.name, c.runtime.name()),
                ));
            }
            let pod = &pods[i - containers.len()];
            // Con l'etichetta «app» si seguono anche i pod ricreati dai rilasci.
            let target = sources::kubernetes::Target {
                namespace: pod.namespace.clone(),
                pod: pod.app.is_none().then(|| pod.name.clone()),
                selector: pod.app.as_ref().map(|a| format!("app={a}")),
                ..Default::default()
            };
            Ok(manual(
                NewSource {
                    kind: "kubernetes".into(),
                    kubernetes: Some(target),
                    ..Default::default()
                },
                labels[i].clone(),
            ))
        }
        3 => {
            let Some((Input::Journal(filter), description, _)) = journal_filter()? else {
                return Ok(None);
            };
            Ok(manual(
                NewSource {
                    kind: "journal".into(),
                    journal: Some(filter),
                    ..Default::default()
                },
                description,
            ))
        }
        _ => {
            page("Un log che non sta in un file", crate::wizard::OTHER_HELP)?;
            Ok(None)
        }
    }
}

/// `loglem add`: il percorso guidato fuori dal menu.
pub fn add(dir: &Path) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("«loglem add» richiede un terminale interattivo. Negli script usa «loglem source add-file» e gli altri «source add-…».");
    }
    let result = add_source(dir, None);
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

/// Una riga scritta dall'utente, fuori dai menu (per le conferme dei comandi).
pub fn ask_line(question: &str) -> Result<String> {
    use std::io::Write;
    print!("{question}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line)
}

/// Il percorso guidato «Aggiungi», in quattro passi come nella pagina web.
/// `site` = aggiungere a un sito già esistente (il nome non si chiede).
fn add_source(dir: &Path, site: Option<&str>) -> Result<()> {
    use crate::wizard;
    let labels: Vec<String> = wizard::CATEGORIES
        .iter()
        .map(|c| format!("{} — {}", c.title, c.description))
        .collect();
    let Some(index) = guided_menu(
        "Aggiungi · 1/4 · Cosa vuoi aggiungere?",
        "Scegli che cosa vuoi tenere d'occhio: poi ti guido passo per passo.",
        &labels,
    )?
    else {
        return Ok(());
    };
    let category = &wizard::CATEGORIES[index];
    let site_name = match site {
        Some(id) => config::load(dir)?
            .sites
            .get(id)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        None => String::new(),
    };
    let name = if !site_name.is_empty() {
        site_name
    } else if category.names_site {
        let Some(name) = field(
            "Aggiungi · Nome",
            &format!(
                "Come si chiama? ({})\nDiventa un sito: ci trovi tutti i suoi log.",
                category.name_hint
            ),
            "",
            |value| {
                if value.trim().is_empty() {
                    Err("Indica un nome.".into())
                } else {
                    Ok(())
                }
            },
        )?
        else {
            return Ok(());
        };
        name
    } else {
        String::new()
    };

    let mut chosen: Vec<wizard::Proposal> = Vec::new();
    let mut selected: Vec<bool> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    if category.id == "other" {
        let Some(p) = pick_location(category.kind)? else {
            return Ok(());
        };
        chosen.push(p);
        selected.push(true);
    } else {
        draw(
            "Aggiungi",
            &["Cerco sul server i programmi e i log…".into()],
            "",
        )?;
        let machine = crate::recipes::Machine::scan();
        // Passo 2: chi lo fa funzionare.
        let mut programs: Vec<String> = Vec::new();
        if category.asks_program {
            let list = wizard::programs(category.id, &name, &machine)?;
            if let (Some(id), Some(question)) = (&list.suggested, &list.question) {
                if choose(question, &["Sì, è lui", "No, scelgo io"])? == Some(0) {
                    programs.push(id.clone());
                }
            }
            while programs.is_empty() || programs.len() < 3 {
                let available: Vec<&wizard::Program> = list
                    .programs
                    .iter()
                    .filter(|p| !programs.contains(&p.id))
                    .collect();
                let mut labels: Vec<String> = available
                    .iter()
                    .map(|p| match p.found.first() {
                        Some(why) => format!("{} · trovato ({why})", p.name),
                        None => p.name.clone(),
                    })
                    .collect();
                labels.push("Un altro programma".into());
                labels.push("Non lo so".into());
                let title = if programs.is_empty() {
                    "Aggiungi · 2/4 · Chi risponde alle richieste?"
                } else {
                    "Aggiungi · 2/4 · Chi c'è davanti (o dietro)?"
                };
                let Some(i) = guided_menu(
                    title,
                    "In cima quelli trovati su questo server.\n«Un altro programma» e «Non lo so» vanno bene: i log li scegli al passo dopo.",
                    &labels,
                )?
                else {
                    return Ok(());
                };
                if i >= available.len() {
                    break;
                }
                programs.push(available[i].id.clone());
                if choose(
                    "Davanti c'è anche un proxy, o dietro un'applicazione?",
                    &["No, continua", "Sì, scegli anche quello"],
                )? != Some(1)
                {
                    break;
                }
            }
        }
        // Passo 3: dove sono i log.
        let found = wizard::proposals(category.id, &name, &programs, &machine)?;
        warnings = found.warnings;
        selected = found.proposals.iter().map(|p| p.exact).collect();
        chosen = found.proposals;
    }
    loop {
        let mut labels: Vec<String> = chosen
            .iter()
            .zip(&selected)
            .map(|(p, on)| {
                let mark = if !p.readable {
                    "[!]"
                } else if *on {
                    "[x]"
                } else {
                    "[ ]"
                };
                let size = p
                    .bytes
                    .map(human_bytes)
                    .map(|s| format!(" · {s}"))
                    .unwrap_or_default();
                format!("{mark} {} · {}{size}", p.title, p.reason)
            })
            .collect();
        labels.push("Manca qualcosa? Aggiungi da…".into());
        let count = selected.iter().filter(|s| **s).count();
        labels.push(format!("Continua con i log scelti ({count})"));
        let mut help = String::from("Invio su un log lo sceglie o lo toglie. [!] = non si legge: aprilo per sapere cosa fare.");
        for w in &warnings {
            help.push_str(&format!("\n⚠ {w}"));
        }
        let Some(i) = guided_menu("Aggiungi · 3/4 · Dove sono i log?", &help, &labels)? else {
            return Ok(());
        };
        if i < chosen.len() {
            let p = &chosen[i];
            if !p.readable {
                page(&p.title, p.problem.as_deref().unwrap_or("Non si legge."))?;
            } else {
                selected[i] = !selected[i];
                if selected[i] && !p.preview.is_empty() {
                    page(
                        &format!("Ultime righe di {}", p.title),
                        &p.preview.join("\n"),
                    )?;
                }
            }
        } else if i == chosen.len() {
            if let Some(p) = pick_location(category.kind)? {
                chosen.push(p);
                selected.push(true);
            }
        } else if count == 0 {
            page(
                "Nessun log scelto",
                "Scegli almeno un log, oppure aggiungilo con «Manca qualcosa?».",
            )?;
        } else {
            break;
        }
    }
    // Passo 4: riepilogo.
    let mut items: Vec<wizard::Proposal> = chosen
        .into_iter()
        .zip(selected)
        .filter(|(_, on)| *on)
        .map(|(p, _)| p)
        .collect();
    loop {
        let mut text = String::new();
        if !name.is_empty() {
            text.push_str(&format!("Sito: {name}\n"));
        } else {
            text.push_str("Va in: Questo server\n");
        }
        for p in &items {
            text.push_str(&format!(
                "\n• {} · {}{}\n  Durata: 28 giorni (predefinita, la cambi nelle impostazioni)",
                wizard::kind_label(p.kind),
                p.title,
                p.program_name
                    .as_deref()
                    .map(|n| format!(" · {n}"))
                    .unwrap_or_default()
            ));
        }
        let Some(choice) = guided_menu(
            "Aggiungi · 4/4 · Riepilogo",
            &text,
            &[
                "Salva e inizia a leggere".into(),
                "Cambia il tipo di un log".into(),
                "Annulla".into(),
            ],
        )?
        else {
            return Ok(());
        };
        match choice {
            0 => break,
            1 => {
                let labels: Vec<String> = items.iter().map(|p| p.title.clone()).collect();
                if let Some(i) = menu("Quale log?", &labels)? {
                    items[i].kind = pick_kind(items[i].kind)?;
                }
            }
            _ => return Ok(()),
        }
    }
    let saved = wizard::save(
        dir,
        wizard::SaveRequest {
            category: category.id.into(),
            name: name.clone(),
            items: items
                .into_iter()
                .map(|p| wizard::SaveItem {
                    kind: Some(p.kind),
                    source: p.source,
                })
                .collect(),
            retention: None,
            site: site.map(str::to_owned),
        },
    )?;
    let mut message = format!(
        "Fatto: {} log collegati{}.\n",
        saved.sources.len(),
        if name.is_empty() {
            " a «Questo server»".to_owned()
        } else {
            format!(" al sito «{name}»")
        }
    );
    match collector::once(dir) {
        Ok(()) => message.push_str("Prima lettura completata: li trovi in «I miei siti»."),
        Err(error) => message.push_str(&format!(
            "La lettura partirà con la raccolta automatica.\nDettaglio: {error:#}"
        )),
    }
    page("Aggiungi", &message)
}

fn search(dir: &Path) -> Result<()> {
    let sources = config::load(dir)?.sources;
    if sources.is_empty() {
        return page(
            "Cerca nei log",
            "Non ci sono ancora sorgenti.\nScegli «I miei siti → Nuovo sito» dal menu principale.",
        );
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
                add_source(dir, None)?;
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
                    s.retention,
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
            format!("Cambia durata: ora {}", source.retention),
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
            "Assegna a un sito".into(),
            "Indietro".into(),
        ];
        match menu(&source.id, &options)? {
            Some(0) => {
                let Some(days) =
                    pick_period("Per quanto tempo vuoi conservare i log?", source.retention)?
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
                    source.retention,
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
                        "Applicazione",
                        "Errori e stack trace",
                        "Database",
                        "Accessi amministrativi",
                        "Sistema",
                        "Altro",
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
                                crate::model::LogKind::Application,
                                crate::model::LogKind::Error,
                                crate::model::LogKind::Database,
                                crate::model::LogKind::AdminAccess,
                                crate::model::LogKind::System,
                                crate::model::LogKind::Other,
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
                let mut options = vec![
                    "Mantieni gli originali (solo lettura)",
                    "Archivia ed elimina file consegnati come conclusi",
                ];
                if matches!(
                    &source.input,
                    Input::File {
                        rotated_pattern: Some(_),
                        ..
                    }
                ) {
                    options.push("Attiva la pulizia dei ruotati verificati e fermi da 24 ore");
                }
                match choose("File originali", &options)? {
                    Some(0) => {
                        crate::management::apply(
                            dir,
                            crate::management::Change {
                                name: source.id.clone(),
                                remove_rotated: Some(false),
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
                    Some(2) => {
                        crate::management::apply(
                            dir,
                            crate::management::Change {
                                name: source.id.clone(),
                                remove_rotated: Some(true),
                                ..Default::default()
                            },
                        )?;
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
            Some(8) => {
                let site = prompt(
                    "Nome o identificatore del sito (vuoto per togliere il sito)",
                    &source.site,
                )?;
                config::assign(dir, &source.id, &site)?;
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
            "Togli il servizio",
        ],
    )?
    else {
        return Ok(());
    };
    if choice == 3 {
        let d = crate::service::details();
        let mut text = if d.failed {
            "Fermo, serve un intervento: il servizio si è fermato con un errore o ha finito i riavvii automatici. Guarda «loglem diagnostics», correggi la causa e riavvialo.".to_owned()
        } else if d.active {
            format!(
                "In esecuzione{}{}: i log vengono letti in automatico.\n{}",
                d.since
                    .as_deref()
                    .map(|s| format!(" dal {s}"))
                    .unwrap_or_default(),
                d.restarts
                    .map(|n| format!(" · riavvii: {n}"))
                    .unwrap_or_default(),
                web_address_text(dir)?
            )
        } else if d.installed {
            "Installato ma fermo: i log vengono letti solo con «Leggi i log adesso». Sceglilo con «Avvia».".to_owned()
        } else {
            "Non installato: Loglem legge i log solo mentre è aperto. Scegli «Installa e avvia il servizio».".to_owned()
        };
        text.push_str(&format!("\n{}", crate::monitor::line(dir)));
        return page("Raccolta automatica", &text);
    }
    if choice == 0 {
        let report = crate::service::install(dir, true)?;
        let text = format!(
            "Fatto. Il servizio raccoglie i log e mostra la pagina web.\n{}\n\n{}",
            web_address_text(dir)?,
            report.text()
        );
        return page("Raccolta automatica", &text);
    }
    if choice == 4 {
        crate::service::uninstall(dir)?;
        return page(
            "Raccolta automatica",
            "Servizio tolto. I dati e le impostazioni di Loglem sono rimasti.\nPer rimetterlo: «Installa e avvia il servizio».",
        );
    }
    let action = match choice {
        1 => ServiceAction::Start,
        _ => ServiceAction::Stop,
    };
    cli::execute(dir, Action::Service { action })?;
    let text = format!(
        "Fatto.\n{}",
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
            "Controllo automatico (acceso, ogni quanti minuti)",
            "Avvisi per mail (facoltativo)",
        ],
    )?
    else {
        return Ok(());
    };
    if choice == 0 {
        return page("Cartella dei dati", &dir.display().to_string());
    }
    if choice == 4 {
        return monitor_menu(dir);
    }
    if choice == 5 {
        return email_menu(dir);
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

fn monitor_menu(dir: &Path) -> Result<()> {
    let s = config::load(dir)?.alerts.monitor;
    let Some(choice) = choose(
        &format!("Controllo automatico · {}", crate::monitor::line(dir)),
        &[
            if s.enabled {
                "Spegni il controllo automatico"
            } else {
                "Accendi il controllo automatico"
            },
            "Cambia ogni quanti minuti",
            "Controlla adesso",
        ],
    )?
    else {
        return Ok(());
    };
    match choice {
        0 => crate::monitor::configure(dir, Some(!s.enabled), None)?,
        1 => {
            let minutes = prompt("Ogni quanti minuti (da 1 a 1440)", &s.minutes.to_string())?;
            crate::monitor::configure(dir, None, Some(minutes.trim().parse()?))?;
        }
        _ => {
            crate::monitor::run(dir)?;
        }
    }
    page("Controllo automatico", &crate::monitor::line(dir))
}

fn email_menu(dir: &Path) -> Result<()> {
    let status = crate::email::status(dir)?;
    let Some(choice) = choose(
        if status.active {
            "Avvisi per mail · attivi"
        } else {
            "Avvisi per mail · spenti"
        },
        &[
            "Mostra come sono configurati",
            "Configura (chiave di Resend, mittente, destinatari)",
            "Invia una mail di prova",
            "Disattiva e cancella la chiave",
        ],
    )?
    else {
        return Ok(());
    };
    match choice {
        0 => {
            let mut text = if status.configured {
                format!(
                    "Mittente: {}\nDestinatari: {}\nLimite: {} mail all'ora\nChiave: {}",
                    status.from,
                    status.to.join(", "),
                    status.hourly_limit,
                    if status.key_set {
                        "salvata (non si può rileggere)"
                    } else {
                        "mancante"
                    }
                )
            } else {
                "Non configurati. Se non li configuri, gli avvisi si vedono solo qui, nella pagina e con «loglem check».".to_owned()
            };
            if let Some(last) = status.last {
                text.push_str(&format!(
                    "\nUltimo invio: {} · {}{}",
                    last.at,
                    if last.ok { "riuscito" } else { "NON riuscito" },
                    last.error.map(|e| format!(" · {e}")).unwrap_or_default()
                ));
            }
            page("Avvisi per mail", &text)
        }
        1 => {
            let from = prompt("Mittente (per esempio noreply@tuodominio.it)", &status.from)?;
            let to = prompt(
                "A chi scrivere (fino a 5 indirizzi, separati da virgola)",
                &status.to.join(", "),
            )?;
            let page_url = prompt(
                "Indirizzo della pagina da scrivere nella mail (facoltativo, Invio per saltare)",
                status.page_url.as_deref().unwrap_or(""),
            )?;
            let note = if status.key_set {
                "Chiave di Resend (Invio per tenere quella salvata): "
            } else {
                "Chiave di Resend (non viene mostrata): "
            };
            let key = crate::access::read_hidden(note)?;
            crate::email::configure(
                dir,
                crate::email::Settings {
                    from,
                    to: to
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    page_url: Some(page_url).filter(|u| !u.trim().is_empty()),
                    hourly_limit: status.hourly_limit,
                },
                Some(&key),
            )?;
            page(
                "Avvisi per mail",
                "Salvato. Scegli «Invia una mail di prova» per controllare che arrivi.",
            )
        }
        2 => {
            crate::email::send_test(dir)?;
            page(
                "Avvisi per mail",
                "Mail di prova partita. Controlla la casella di posta.",
            )
        }
        _ => {
            crate::email::disable(dir)?;
            page(
                "Avvisi per mail",
                "Spenti. La chiave è stata cancellata da questo computer.",
            )
        }
    }
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
