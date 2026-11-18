use crate::{
    archive::Archive,
    collector, config,
    model::{Input, Source, UpdateSettings},
    service, sources, terminal, timestamps, update, util,
};
use anyhow::{bail, Context, Result};
use std::io::IsTerminal;
#[path = "commands.rs"]
mod commands;
use std::{fs, path::PathBuf};

#[derive(Debug)]
pub struct Cli {
    pub data_dir: Option<PathBuf>,
    pub command: Option<Action>,
}

#[derive(Debug)]
pub enum Action {
    /// Read persistent Loglem errors, cause chains, and stack traces.
    Diagnostics,
    /// Raccoglie i log e apre l'interfaccia web (se il servizio è attivo, ne mostra l'indirizzo).
    Web,
    /// Open the keyboard-driven terminal interface (also the default).
    Tui,
    /// Il percorso guidato «Aggiungi» nel terminale.
    Add,
    /// Create an empty configuration.
    Init,
    /// Create, list or inspect sites and projects.
    Site {
        action: SiteAction,
    },
    /// Configure sources without opening an interface.
    Source {
        action: SourceAction,
    },
    /// List Podman containers and their pods.
    PodmanList {
        connection: Option<String>,
    },
    /// Cerca i log nella configurazione dei programmi conosciuti (src/recipes).
    Discover,
    /// Elenca i container Docker.
    DockerList {
        context: Option<String>,
    },
    /// Elenca i pod di Kubernetes in tutti i namespace.
    KubernetesList {
        context: Option<String>,
    },
    /// Show archive locations, sizes, dates, and collection status.
    Status {
        json: bool,
    },
    /// Collect once and enforce archive retention.
    Collect,
    /// Controlla che Loglem giri e che le sorgenti vengano lette. Esce con codice 1 se c'è
    /// un problema: utilizzabile da cron, systemd o un programma di monitoraggio.
    Check {
        /// Una sorgente senza letture riuscite da più minuti di così è segnalata.
        minutes: u64,
    },
    /// Mostra le righe nuove appena arrivano (in locale, o da un altro computer con --server).
    Tail {
        /// Solo questa sorgente.
        source: Option<String>,
        query: crate::query::Query,
        /// Indirizzo della pagina web del server, es. http://10.0.0.1:8790
        server: Option<String>,
        /// Utente con cui entrare nel server (la password viene chiesta).
        user: Option<String>,
        /// Quante righe recenti mostrare prima della diretta (solo in locale).
        history: usize,
    },
    /// Copia coerente di configurazione e archivi in una cartella nuova.
    Backup {
        target: PathBuf,
    },
    /// Verifica un backup: impronte, integrità SQLite, righe. Esce con codice 1 se non è valido.
    BackupVerify {
        target: PathBuf,
    },
    /// Ripristina un backup verificato (Loglem deve essere fermo).
    Restore {
        target: PathBuf,
        /// Sostituisce i dati attuali.
        replace: bool,
    },
    /// Cancella i dati di una persona (GDPR art. 17) da tutte le sorgenti. Senza --apply mostra
    /// solo quante righe toccherebbe. Non tocca i file originali né i backup.
    Erase {
        source: Option<String>,
        query: crate::query::Query,
        /// Perché si cancella, per il registro (es. «richiesta art. 17 del 17/11/2026, pratica 3»).
        reason: Option<String>,
        /// Cancella anche dai file originali (solo file finiti e già archiviati).
        originals: bool,
        /// Esegue davvero la cancellazione.
        apply: bool,
    },
    /// Esporta i dati di una persona (GDPR art. 15) in un file di testo o JSON.
    Export {
        source: Option<String>,
        query: crate::query::Query,
        /// File da creare (solo il proprietario lo legge). Senza, scrive a video.
        output: Option<PathBuf>,
        json: bool,
    },
    /// Chi ha cercato, esportato o cancellato, e quando (ultimi giorni).
    Audit {
        days: u32,
    },
    /// Statistiche di un log di accesso: richieste per giorno, pagine, codici, IP, lentezza.
    Stats {
        source: String,
        days: u32,
        json: bool,
    },
    /// Cosa c'è nel log di una sorgente (IP, email, credenziali…) e cosa Loglem ti suggerisce,
    /// con i riferimenti di legge. Sono suggerimenti: verificali
    Advice {
        source: String,
        /// Applica la proposta di un suggerimento (es. --apply ip)
        apply: Option<String>,
        /// Registra una scelta: ID=done, ID=declined, ID=later o ID=reopen (es. --choose notice=done)
        choose: Option<String>,
        json: bool,
    },
    /// Modalità prova: sigillo giornaliero con impronte concatenate e verifica.
    Proof {
        action: ProofAction,
    },
    /// Dati personali: mascheramento degli IP, campi JSON da tenere, rapporto.
    Privacy {
        action: PrivacyAction,
    },
    /// Allarmi: regole, comando di notifica, «segna come visto».
    Alerts {
        action: AlertsAction,
    },
    /// Accesso alla pagina web da altri computer: utenti e indirizzo.
    Access {
        action: AccessAction,
    },
    /// Servizio: raccoglie i log e serve l'interfaccia web finché non riceve SIGINT o SIGTERM.
    Serve,
    /// Literal, case-sensitive text search; dates are UTC unless a timezone is supplied.
    Search(SearchArgs),
    /// Preview expired entries, or delete them with --apply.
    Prune {
        apply: bool,
    },
    /// Archive a file explicitly handed off by a producer that has CLOSED it.
    ArchiveCompleted {
        source: String,
        file: PathBuf,
        /// Required: the producer will never write to this file again.
        producer_closed: bool,
        /// Remove the completed original only after byte-for-byte verification.
        remove: bool,
    },
    Config {
        action: ConfigAction,
    },
    Service {
        action: ServiceAction,
    },
    Update {
        action: UpdateAction,
    },
    /// Test the executable and embedded SQLite schema without changing user data.
    SelfCheck,
    ApplyUpdate {
        stage: PathBuf,
    },
}

#[derive(Debug)]
pub enum SiteAction {
    Add {
        name: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Remove {
        id: String,
        with_logs: bool,
    },
    List,
    Show {
        id: String,
    },
    /// Count original locations and archives, and compare collected samples.
    Copies {
        id: String,
    },
    /// Compare the latest 200 rows per source. Include local files and services with --external.
    Duplicates {
        id: String,
        external: bool,
    },
}

#[derive(Debug)]
pub enum SourceAction {
    /// Assign a source to a site. Omit the site to clear the assignment.
    AssignSite {
        name: String,
        site: String,
    },
    /// Change display name, calendar retention, archive location, or source connection.
    Configure {
        name: String,
        label: Option<String>,
        amount: Option<u32>,
        unit: Option<crate::model::RetentionUnit>,
        log_kind: Option<crate::model::LogKind>,
        archive_dir: Option<String>,
        completed_dir: Option<String>,
        handoff_confirmed: bool,
        /// true: elimina i file ruotati dopo averli archiviati e verificati.
        remove_rotated: Option<bool>,
        path: Option<PathBuf>,
        container: Option<String>,
        connection: Option<String>,
    },
    Rename {
        name: String,
        label: String,
    },
    Pause {
        name: String,
    },
    Resume {
        name: String,
    },
    Detach {
        name: String,
    },
    /// Elimina una sorgente in pausa o scollegata, con tutto il suo storico.
    Remove {
        name: String,
    },
    AddFolder {
        /// Site name or identifier.
        site: String,
        name: String,
        path: PathBuf,
        days: u32,
    },
    AddFile {
        /// Site name or identifier.
        site: String,
        name: String,
        path: PathBuf,
        days: u32,
        rotated_pattern: Option<String>,
    },
    /// Journal di systemd, eventualmente solo alcuni servizi o programmi.
    AddJournal {
        /// Site name or identifier.
        site: String,
        name: String,
        /// Servizio systemd da leggere (ripetibile), es. --unit caddy.service
        units: Vec<String>,
        /// Programma syslog da leggere (ripetibile), es. --identifier sshd
        identifiers: Vec<String>,
        /// Campo del journal da confrontare (ripetibile), es. --match CONTAINER_NAME=web
        matches: Vec<String>,
        /// Un container Podman per nome: continua anche quando viene ricreato.
        container: Option<String>,
        /// Solo voci fino a questa gravità: 0 emergenza … 3 errore … 7 debug
        priority: Option<u8>,
        days: u32,
    },
    /// Log di accesso di Caddy: legge il file attivo e i file ruotati.
    AddCaddy {
        /// Site name or identifier.
        site: String,
        name: String,
        /// Il file su cui scrive Caddy, es. /var/log/caddy/access.log
        path: PathBuf,
        days: u32,
    },
    /// Container Docker (nome o identificativo).
    AddDocker {
        /// Site name or identifier.
        site: String,
        name: String,
        container: String,
        context: Option<String>,
        days: u32,
    },
    /// Kubernetes: un pod (--pod) oppure i pod con un'etichetta (--selector app=web),
    /// che continua a funzionare quando i pod vengono ricreati.
    AddKubernetes {
        /// Site name or identifier.
        site: String,
        name: String,
        namespace: String,
        pod: Option<String>,
        selector: Option<String>,
        container: Option<String>,
        context: Option<String>,
        days: u32,
    },
    AddPodman {
        /// Site name or identifier.
        site: String,
        name: String,
        container: String,
        days: u32,
        connection: Option<String>,
    },
    List,
    Set {
        name: String,
        days: Option<u32>,
        enabled: Option<bool>,
    },
}

#[derive(Debug, Default)]
pub struct SearchArgs {
    pub source: Option<String>,
    pub query: crate::query::Query,
    pub from: Option<String>,
    /// YYYY-MM-DD includes that entire day; a full timestamp is an exclusive upper bound.
    pub to: Option<String>,
    pub limit: u16,
    pub json: bool,
}

#[derive(Debug)]
pub enum ConfigAction {
    Export {
        output: PathBuf,
    },
    Import {
        file: PathBuf,
    },
    Poll {
        seconds: u64,
    },
    /// Porta locale fissa dell'interfaccia web (vale dal prossimo avvio).
    WebPort {
        port: u16,
    },
}
#[derive(Debug)]
pub enum ProofAction {
    /// Attiva la modalità prova per una sorgente.
    Enable {
        source: String,
        /// Autorità di marca temporale RFC 3161, es. https://freetsa.org/tsr
        tsa: Option<String>,
    },
    /// Disattiva la modalità prova (il registro già scritto resta).
    Disable { source: String },
    /// Verifica i giorni sigillati. Con --archive e --chain verifica una copia.
    Verify {
        source: Option<String>,
        archive: Option<PathBuf>,
        chain: Option<PathBuf>,
    },
}

#[derive(Debug)]
pub enum PrivacyAction {
    /// Imposta le tutele di una sorgente. --mask-ips-after-days 0 = subito; vuoto = mai.
    Set {
        source: String,
        mask_ips_after_days: Option<u32>,
        /// Smette di mascherare gli IP.
        no_mask: bool,
        /// Campi JSON da tenere, separati da virgola (es. request.uri,status). Vuoto = tutti.
        keep_fields: Option<String>,
    },
    /// Cosa si conserva, per quanto e con quali tutele (per il registro delle conservazioni).
    Report,
}

#[derive(Debug)]
pub enum AlertsAction {
    /// Elenca regole, comando di notifica e problemi attuali con il loro nome.
    List,
    /// Mostra o cambia il controllo automatico (acceso/spento, ogni quanti minuti).
    Monitor {
        enabled: Option<bool>,
        minutes: Option<u32>,
    },
    /// Attiva gli avvisi per mail con Resend. La chiave si legge dalla tastiera.
    EmailSet {
        from: String,
        to: Vec<String>,
        page_url: Option<String>,
        hourly_limit: Option<u32>,
    },
    /// Manda una mail di prova.
    EmailTest,
    /// Spegne gli avvisi per mail e cancella la chiave.
    EmailOff,
    /// Mostra come sono configurati gli avvisi per mail (mai la chiave).
    EmailShow,
    /// Allarme se un testo compare almeno COUNT volte in MINUTES minuti.
    AddPattern {
        name: String,
        text: String,
        count: u32,
        minutes: u32,
        source: Option<String>,
    },
    /// Allarme se l'archivio supera MEGABYTES.
    AddSize {
        name: String,
        megabytes: u64,
        source: Option<String>,
    },
    /// Allarme se sul disco dei dati restano meno di MEGABYTES liberi.
    AddFreeSpace {
        name: String,
        megabytes: u64,
    },
    Remove {
        name: String,
    },
    /// Comando da eseguire quando i problemi cambiano (riceve LOGLEM_ALERTS). Vuoto = nessuno.
    Command {
        command: String,
    },
    /// Segna un problema come visto (il nome si legge con «alerts list»).
    Seen {
        id: String,
    },
}

#[derive(Debug)]
pub enum AccessAction {
    /// Crea un utente, o cambia password e ruolo di uno esistente. La password viene chiesta.
    UserAdd {
        name: String,
        role: crate::access::Role,
    },
    /// Elimina un utente; le sue sessioni aperte si chiudono subito.
    UserRemove { name: String },
    /// Elenca gli utenti.
    UserList,
    /// Pubblica la pagina dietro un reverse proxy (Caddy, Nginx…). Richiede almeno un utente.
    Proxy {
        /// Indirizzo del proxy da cui accettare X-Forwarded-For/-Proto (ripetibile).
        trust: Vec<std::net::IpAddr>,
        /// Prefisso se il proxy non lo toglie, es. /loglem. Vuoto = nessuno.
        base_path: Option<String>,
    },
    /// Dove ascoltare: «local» (solo questo computer), un IP di questo computer
    /// (es. quello della VPN) o 0.0.0.0. Vale dal prossimo avvio.
    Address { address: String },
}
#[derive(Debug)]
pub enum ServiceAction {
    Install {
        start: bool,
    },
    /// Toglie il servizio (i dati restano).
    Uninstall,
    Start,
    Stop,
    Status,
}
#[derive(Debug)]
pub enum UpdateAction {
    Configure {
        repository: String,
        public_key: String,
    },
    Check,
    Install,
    Status,
}

pub(crate) use commands::command as parse_command;

pub fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, Some(Action::SelfCheck)) {
        let db = rusqlite::Connection::open_in_memory()?;
        db.execute_batch(include_str!("schema.sql"))?;
        crate::advice::check_catalogs()?;
        let recipes = crate::recipes::validate_all()?;
        println!(
            "Loglem {}: executable, SQLite schema and {recipes} recipes OK",
            crate::VERSION
        );
        return Ok(());
    }
    let dir = match cli.data_dir {
        Some(dir) => dir,
        None => config::default_directory()?,
    };
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()?.join(dir)
    };
    util::private_dir(&dir)?;
    crate::diagnostics::init(&dir)?;
    let result = execute(&dir, cli.command.unwrap_or(Action::Tui));
    if let Err(error) = &result {
        crate::diagnostics::error("cli", "execute", None, error);
    }
    result
}

pub fn execute(dir: &std::path::Path, action: Action) -> Result<()> {
    match action {
        Action::Diagnostics => {
            println!(
                "{}",
                serde_json::to_string_pretty(&crate::diagnostics::recent(dir)?)?
            );
            Ok(())
        }
        Action::Web => crate::web::serve(dir),
        Action::Tui => terminal::run(dir),
        Action::Add => terminal::add(dir),
        Action::Init => {
            config::edit(dir, |_| Ok(()))?;
            println!("Configuration: {}", dir.join("config.json").display());
            Ok(())
        }
        Action::Site { action } => {
            match action {
                SiteAction::Add { name } => println!("{}", config::add_site(dir, &name)?),
                SiteAction::Rename { id, name } => {
                    config::rename_site(dir, &id, &name)?;
                    println!("Sito rinominato in «{name}».");
                }
                SiteAction::Remove { id, with_logs } => {
                    let (_, logs) = config::site_sources(dir, &id)?;
                    if with_logs && !logs.is_empty() {
                        println!(
                            "Elimino il sito e {} log con tutto il loro storico: {}",
                            logs.len(),
                            logs.join(", ")
                        );
                        if std::io::stdin().is_terminal() {
                            let answer =
                                crate::terminal::ask_line("Scrivi «elimina» per confermare: ")?;
                            anyhow::ensure!(
                                answer.trim() == "elimina",
                                "Annullato: niente è stato eliminato"
                            );
                        }
                    }
                    let removed = config::remove_site(dir, &id, with_logs)?;
                    println!("Sito eliminato ({} log tolti).", removed.len());
                }
                SiteAction::List => {
                    for site in config::load(dir)?.sites.values() {
                        println!("{}\t{}", site.id, util::safe_text(&site.name));
                    }
                }
                SiteAction::Copies { id }
                | SiteAction::Duplicates {
                    id,
                    external: false,
                } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&crate::copies::report(dir, &id, false)?)?
                    );
                }
                SiteAction::Duplicates { id, external: true } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&crate::copies::report(dir, &id, true)?)?
                    );
                }
                SiteAction::Show { id } => {
                    let collector::SiteStatus {
                        sites,
                        sources: rows,
                        ..
                    } = collector::statuses_with_sites(dir)?;
                    let site = sites.get(&id).context("Sito inesistente")?;
                    let logs: Vec<_> = rows.into_iter().filter(|s| s.site == id).collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({"site": site, "logs": logs})
                        )?
                    );
                }
            }
            Ok(())
        }
        Action::Source { action } => {
            match action {
                SourceAction::AssignSite { name, site } => config::assign(dir, &name, &site)?,
                SourceAction::Configure {
                    name,
                    label,
                    amount,
                    unit,
                    log_kind,
                    archive_dir,
                    completed_dir,
                    handoff_confirmed,
                    remove_rotated,
                    path,
                    container,
                    connection,
                } => {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name,
                            label,
                            retention: amount
                                .zip(unit)
                                .map(|(amount, unit)| crate::model::Retention { amount, unit }),
                            log_kind,
                            archive_dir,
                            completed_dir,
                            handoff_confirmed,
                            remove_rotated,
                            path,
                            container,
                            connection,
                            ..Default::default()
                        },
                    )?;
                }
                SourceAction::Rename { name, label } => {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name,
                            label: Some(label),
                            ..Default::default()
                        },
                    )?;
                }
                SourceAction::Pause { name } => {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name,
                            enabled: Some(false),
                            ..Default::default()
                        },
                    )?;
                }
                SourceAction::Resume { name } => {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name,
                            enabled: Some(true),
                            ..Default::default()
                        },
                    )?;
                }
                SourceAction::Detach { name } => {
                    crate::management::apply(
                        dir,
                        crate::management::Change {
                            name,
                            detached: Some(true),
                            ..Default::default()
                        },
                    )?;
                }
                SourceAction::Remove { name } => {
                    crate::management::remove(dir, &name)?;
                    crate::audit::record(
                        dir,
                        "console",
                        "elimina sorgente",
                        &format!("sorgente: {name}"),
                    );
                    println!("Sorgente {name} eliminata, con il suo storico.");
                }
                SourceAction::AddFolder {
                    site,
                    name,
                    path,
                    days,
                } => {
                    let path = fs::canonicalize(path).context("Choose an existing folder")?;
                    if !path.is_dir() {
                        bail!("Choose a folder");
                    }
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            enabled: true,
                            input: Input::Folder {
                                path,
                                pattern: None,
                            },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddJournal {
                    site,
                    name,
                    units,
                    identifiers,
                    mut matches,
                    container,
                    priority,
                    days,
                } => {
                    matches.extend(container.map(|c| format!("CONTAINER_NAME={c}")));
                    let filter = crate::sources::journal::Filter {
                        units,
                        identifiers,
                        matches,
                        priority,
                    };
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            input: Input::Journal(filter),
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddCaddy {
                    site,
                    name,
                    path,
                    days,
                } => {
                    let retention = crate::model::Retention {
                        amount: days,
                        unit: crate::model::RetentionUnit::Days,
                    };
                    // Comando deprecato: un log di accesso è un file come gli altri.
                    let mut source = crate::management::file_source(name, &path, retention)?;
                    source.site = site;
                    source.log_kind = crate::model::LogKind::Access;
                    config::add(dir, source)?;
                    eprintln!("Nota: «source add-caddy» è deprecato, usa «source add-file».");
                    println!("Sorgente aggiunta.");
                }
                SourceAction::AddFile {
                    site,
                    name,
                    path,
                    days,
                    rotated_pattern,
                } => {
                    let path = fs::canonicalize(path).context("Choose an existing .log file")?;
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            enabled: true,
                            input: Input::File {
                                path,
                                rotated_pattern,
                            },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddPodman {
                    site,
                    name,
                    container,
                    days,
                    connection,
                } => {
                    let found = sources::podman::discover(connection.as_deref())?
                        .into_iter()
                        .find(|c| c.id == container || c.name == container)
                        .context("Container not found in the selected Podman connection")?;
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            enabled: true,
                            input: Input::Podman {
                                container: found.id,
                                connection,
                            },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddDocker {
                    site,
                    name,
                    container,
                    context,
                    days,
                } => {
                    let runtime = sources::podman::Runtime::Docker;
                    let found = sources::podman::discover_with(runtime, context.as_deref())?
                        .into_iter()
                        .find(|c| c.id == container || c.name == container)
                        .context("Container Docker non trovato")?;
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            input: Input::Docker {
                                container: found.id,
                                context,
                            },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddKubernetes {
                    site,
                    name,
                    namespace,
                    pod,
                    selector,
                    container,
                    context,
                    days,
                } => {
                    let target = sources::kubernetes::Target {
                        namespace,
                        pod,
                        selector,
                        container,
                        context,
                    };
                    target.validate()?;
                    config::add(
                        dir,
                        Source {
                            id: name,
                            site,
                            retention: crate::model::Retention::days(days),
                            input: Input::Kubernetes(target),
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::List => println!(
                    "{}",
                    serde_json::to_string_pretty(&config::load(dir)?.sources)?
                ),
                SourceAction::Set {
                    name,
                    days,
                    enabled,
                } => config::edit(dir, |config| {
                    let source = config
                        .sources
                        .iter_mut()
                        .find(|s| s.id == name)
                        .context("Source not found")?;
                    if let Some(days) = days {
                        source.retention = crate::model::Retention::days(days);
                    }
                    if let Some(enabled) = enabled {
                        source.enabled = enabled;
                    }
                    Ok(())
                })?,
            }
            Ok(())
        }
        Action::PodmanList { connection } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&sources::podman::discover(connection.as_deref())?)?
            );
            Ok(())
        }
        Action::Discover => {
            let found = crate::discover::find_all();
            if found.is_empty() {
                println!("Nessuna configurazione di Caddy, Nginx o Apache trovata.");
            }
            for item in found {
                let missing = if item.exists {
                    ""
                } else {
                    " (il file non esiste ancora)"
                };
                println!(
                    "{} · {}{missing}\n  trovato in {}",
                    item.program,
                    item.path.display(),
                    item.from.display()
                );
            }
            Ok(())
        }
        Action::DockerList { context } => {
            let containers = sources::podman::discover_with(
                sources::podman::Runtime::Docker,
                context.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&containers)?);
            Ok(())
        }
        Action::KubernetesList { context } => {
            let pods = sources::kubernetes::discover(context.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&pods)?);
            Ok(())
        }
        Action::Status { json } => {
            let statuses = collector::statuses(dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&statuses)?);
            } else {
                println!("{}\n", crate::monitor::line(dir));
                println!("{}", terminal::status_text(&statuses));
            }
            Ok(())
        }
        Action::Collect => collector::once(dir),
        Action::Check { minutes } => {
            let problems =
                crate::alerts::hide_acknowledged(dir, crate::health::check(dir, minutes, true)?);
            if problems.is_empty() {
                println!("Tutto a posto: Loglem è in funzione e tutte le sorgenti vengono lette.");
                return Ok(());
            }
            for problem in &problems {
                println!("- {}", problem.text);
            }
            match problems.len() {
                1 => bail!("1 problema trovato"),
                n => bail!("{n} problemi trovati"),
            }
        }
        Action::Tail {
            source,
            query,
            server,
            user,
            history,
        } => match (server, user) {
            (Some(server), Some(user)) => {
                crate::tail::remote(&server, &user, source.as_deref(), &query)
            }
            _ => crate::tail::local(dir, source.as_deref(), &query, history),
        },
        Action::Access { action } => access_command(dir, action),
        Action::Alerts { action } => alerts_command(dir, action),
        Action::Erase {
            source,
            query,
            reason,
            originals,
            apply,
        } => {
            let counts = crate::erase::preview(dir, source.as_deref(), &query)?;
            for (name, count) in &counts {
                println!("{name}: {count} righe nell'archivio di Loglem");
            }
            if originals {
                for (name, files) in
                    crate::erase::preview_originals(dir, source.as_deref(), &query)?
                {
                    print_originals(&name, &files);
                }
            }
            let total: i64 = counts.iter().map(|c| c.1).sum();
            if !apply {
                println!("Totale: {total} righe. Non è stato cancellato nulla: per farlo aggiungi --apply --reason \"…\".");
                return Ok(());
            }
            let user = terminal_user();
            let done = crate::erase::erase(
                dir,
                source.as_deref(),
                &query,
                reason.as_deref().unwrap_or(""),
                &user,
                originals,
            )?;
            println!(
                "Cancellate {} righe da {} sorgenti.",
                done.rows, done.sources
            );
            for (name, files) in &done.originals {
                print_originals(name, files);
            }
            if done.resealed_days > 0 {
                println!(
                    "Modalità prova: ricalcolate le impronte di {} giorni e scritta la nota nel registro.",
                    done.resealed_days
                );
            }
            println!(
                "Restano: {}i log di Podman, Docker, Kubernetes e del journal, e i backup già fatti \
                 (finché non scadono). Cancellali a parte se serve.",
                if originals {
                    "i file che non si potevano riscrivere (vedi sopra), "
                } else {
                    "i file originali dei programmi (usa --originals per toglierli), "
                }
            );
            Ok(())
        }
        Action::Export {
            source,
            query,
            output,
            json,
        } => {
            let user = terminal_user();
            match output {
                Some(path) => {
                    let mut options = fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut file = options.open(&path).with_context(|| {
                        format!("Non posso creare {} (esiste già?)", path.display())
                    })?;
                    let n = crate::erase::export(
                        dir,
                        source.as_deref(),
                        &query,
                        &mut file,
                        json,
                        &user,
                    )?;
                    println!("Esportate {n} righe in {}", path.display());
                }
                None => {
                    let stdout = std::io::stdout();
                    crate::erase::export(
                        dir,
                        source.as_deref(),
                        &query,
                        &mut stdout.lock(),
                        json,
                        &user,
                    )?;
                }
            }
            Ok(())
        }
        Action::Audit { days } => {
            for entry in crate::audit::recent(dir, days)? {
                println!(
                    "{}  {:<14} {:<14} {}",
                    entry["at"].as_str().unwrap_or(""),
                    entry["user"].as_str().unwrap_or(""),
                    entry["action"].as_str().unwrap_or(""),
                    entry["detail"].as_str().unwrap_or("")
                );
            }
            Ok(())
        }
        Action::Stats { source, days, json } => {
            let stats = crate::stats::compute(dir, &source, days)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                print!("{}", crate::stats::text(&stats));
            }
            Ok(())
        }
        Action::Advice {
            source,
            apply,
            choose,
            json,
        } => {
            let language = crate::advice::DEFAULT_LANGUAGE;
            if let Some(id) = apply {
                crate::advice::apply(dir, &source, &id, "console")?;
                println!("Proposta «{id}» applicata.");
            } else if let Some(choice) = choose {
                let (id, choice) = choice
                    .split_once('=')
                    .context("Scrivi ID=done, ID=declined o ID=later")?;
                crate::advice::choose(dir, &source, id, choice, "console")?;
                println!("Scelta registrata.");
            } else if json {
                let report = crate::advice::report(dir, &source, language)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", crate::advice::text(dir, &source, language)?);
            }
            Ok(())
        }
        Action::Proof { action } => match action {
            ProofAction::Enable { source, tsa } => config::edit(dir, |c| {
                let target = c
                    .sources
                    .iter_mut()
                    .find(|s| s.id == source)
                    .context("Sorgente non trovata")?;
                target.proof = Some(crate::proof::Proof { tsa });
                Ok(())
            }),
            ProofAction::Disable { source } => config::edit(dir, |c| {
                let target = c
                    .sources
                    .iter_mut()
                    .find(|s| s.id == source)
                    .context("Sorgente non trovata")?;
                target.proof = None;
                Ok(())
            }),
            ProofAction::Verify {
                source,
                archive,
                chain,
            } => {
                let report = match (source, archive, chain) {
                    (_, Some(archive), Some(chain)) => crate::proof::verify(&archive, &chain)?,
                    (Some(source), _, _) => crate::proof::verify_source(dir, &source)?,
                    _ => bail!("Indica una sorgente, oppure --archive e --chain"),
                };
                println!("{}", report.report);
                if report.problems > 0 {
                    bail!("la verifica della prova non è superata");
                }
                Ok(())
            }
        },
        Action::Privacy { action } => match action {
            PrivacyAction::Report => {
                println!("{}", crate::privacy::report(dir)?);
                Ok(())
            }
            PrivacyAction::Set {
                source,
                mask_ips_after_days,
                no_mask,
                keep_fields,
            } => config::edit(dir, |c| {
                let target = c
                    .sources
                    .iter_mut()
                    .find(|s| s.id == source)
                    .context("Sorgente non trovata")?;
                if no_mask {
                    target.privacy.mask_ips_after_days = None;
                } else if mask_ips_after_days.is_some() {
                    target.privacy.mask_ips_after_days = mask_ips_after_days;
                }
                if let Some(fields) = keep_fields {
                    target.privacy.keep_fields = fields
                        .split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                Ok(())
            }),
        },
        Action::Backup { target } => {
            println!("{}", crate::backup::create(dir, &target)?);
            Ok(())
        }
        Action::BackupVerify { target } => {
            println!("{}", crate::backup::verify(&target)?);
            Ok(())
        }
        Action::Restore { target, replace } => {
            println!("{}", crate::backup::restore(dir, &target, replace)?);
            Ok(())
        }
        Action::Serve => crate::web::serve(dir),
        Action::Search(args) => {
            let from = args
                .from
                .as_deref()
                .map(|s| timestamps::parse_date(s, false))
                .transpose()?
                .unwrap_or(i64::MIN);
            let until = args
                .to
                .as_deref()
                .map(|s| timestamps::parse_date(s, true))
                .transpose()?
                .unwrap_or(i64::MAX);
            crate::audit::record(
                dir,
                &terminal_user(),
                "ricerca",
                &format!(
                    "sorgente: {}; filtri: {}",
                    args.source.as_deref().unwrap_or("tutte"),
                    crate::audit::filter_kinds(&args.query)
                ),
            );
            let rows = collector::search(
                dir,
                args.source.as_deref(),
                &args.query,
                from,
                until,
                args.limit as usize,
            )?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for row in rows {
                    println!(
                        "{} [{} / {}] {}",
                        row.timestamp,
                        util::safe_text(&row.source),
                        row.timestamp_origin,
                        util::safe_text(&row.message)
                    );
                }
            }
            Ok(())
        }
        Action::Prune { apply } => {
            let _lock = util::Lock::acquire(&dir.join("collector.lock"))?;
            let now = timestamps::now_ns();
            for source in config::load(dir)?.sources {
                let path = config::archive_path(dir, &source.id);
                if !path.exists() {
                    continue;
                }
                if source.proof.is_some() {
                    println!(
                        "{}: modalità prova, scade solo per giorni interi sigillati",
                        source.id
                    );
                    continue;
                }
                let mut archive = Archive::open(&path)?;
                let cutoff = source.retention.cutoff(now);
                let count = if apply {
                    archive.retain_since(cutoff)? as i64
                } else {
                    archive.db.query_row(
                        "SELECT count(*) FROM log_rows WHERE event_ns<?1 OR collected_ns<?1",
                        [cutoff],
                        |r| r.get(0),
                    )?
                };
                println!(
                    "{}: {count} {} entries",
                    source.id,
                    if apply {
                        "deleted"
                    } else {
                        "expired (preview)"
                    }
                );
            }
            Ok(())
        }
        Action::ArchiveCompleted {
            source,
            file,
            producer_closed,
            remove,
        } => {
            if !producer_closed {
                bail!(concat!(
                    "Confirm the producer has closed this file and will never write it again ",
                    "using --producer-closed. A dated filename alone is not proof"
                ));
            }
            let _lock = util::Lock::acquire(&dir.join("collector.lock"))?;
            let source = config::source(dir, &source)?;
            let Input::File { path, .. } = &source.input else {
                bail!("This command applies only to file sources");
            };
            let mut archive = Archive::open(&config::archive_path(dir, &source.id))?;
            let count = sources::file::archive_completed(&mut archive, path, &file, remove)?;
            println!(
                "Verified completed file: {count} new entries; original {}",
                if remove { "removed" } else { "preserved" }
            );
            Ok(())
        }
        Action::Config { action } => {
            match action {
                ConfigAction::Export { output } => {
                    util::atomic_write(&output, &serde_json::to_vec_pretty(&config::load(dir)?)?)?
                }
                ConfigAction::Import { file } => {
                    if fs::metadata(&file)?.len() > config::MAX_CONFIG_BYTES {
                        bail!("Configuration file is too large");
                    }
                    let imported = config::decode(serde_json::from_slice(&fs::read(file)?)?)?;
                    config::edit(dir, |c| {
                        *c = imported;
                        Ok(())
                    })?;
                }
                ConfigAction::Poll { seconds } => config::edit(dir, |c| {
                    c.poll_seconds = seconds;
                    Ok(())
                })?,
                ConfigAction::WebPort { port } => {
                    if port < 1024 {
                        bail!("Usa una porta da 1024 in su: le altre richiedono privilegi di amministratore");
                    }
                    config::edit(dir, |c| {
                        c.web_port = Some(port);
                        Ok(())
                    })?
                }
            }
            Ok(())
        }
        Action::Service { action } => match action {
            ServiceAction::Install { start } => {
                println!("{}", service::install(dir, start)?.text());
                Ok(())
            }
            ServiceAction::Uninstall => {
                service::uninstall(dir)?;
                println!("Servizio tolto. I dati e le impostazioni di Loglem sono rimasti.");
                Ok(())
            }
            ServiceAction::Start => service::start(),
            ServiceAction::Stop => service::stop(),
            ServiceAction::Status => {
                println!(
                    "{}",
                    if service::is_active()? {
                        "In esecuzione"
                    } else {
                        "Ferma"
                    }
                );
                Ok(())
            }
        },
        Action::Update { action } => {
            match action {
                UpdateAction::Configure {
                    repository,
                    public_key,
                } => config::edit(dir, |c| {
                    c.updates = Some(UpdateSettings {
                        repository,
                        public_key,
                    });
                    Ok(())
                })?,
                UpdateAction::Check => {
                    let manifest = update::check(dir)?;
                    println!(
                        "Installed: {}\nLatest signed release: {}",
                        crate::VERSION,
                        manifest.version
                    );
                }
                UpdateAction::Install => {
                    let stage = update::prepare(dir)?;
                    update::launch(dir, &stage)?;
                    println!(
                        "Independent updater started. Use 'loglem update status' for progress."
                    );
                }
                UpdateAction::Status => println!(
                    "{}",
                    util::safe_text(
                        &fs::read_to_string(dir.join("updates/status.txt"))
                            .unwrap_or_else(|_| "No update has been requested".into())
                    )
                ),
            }
            Ok(())
        }
        Action::ApplyUpdate { stage } => update::apply(dir, &stage),
        Action::SelfCheck => unreachable!(),
    }
}

fn access_command(dir: &std::path::Path, action: AccessAction) -> Result<()> {
    use crate::access;
    match action {
        AccessAction::UserAdd { name, role } => {
            access::validate_name(&name)?;
            let password = access::read_password(&format!(
                "Password per {name} (almeno {} caratteri): ",
                access::MIN_PASSWORD
            ))?;
            if std::env::var_os("LOGLEM_PASSWORD").is_none()
                && access::read_password("Ripeti la password: ")? != password
            {
                bail!("Le due password non coincidono");
            }
            let created = access::save_user(dir, &name, role, &password)?;
            println!(
                "{} {name}. Dalla prossima apertura la pagina chiederà nome e password.",
                if created {
                    "Creato l'utente"
                } else {
                    "Aggiornato l'utente"
                }
            );
        }
        AccessAction::UserRemove { name } => {
            access::remove_user(dir, &name)?;
            println!("Utente {name} eliminato.");
        }
        AccessAction::UserList => {
            let config = config::load(dir)?;
            if config.access.users.is_empty() {
                println!(
                    "Nessun utente: la pagina risponde solo su questo computer, senza password."
                );
            }
            for user in &config.access.users {
                let role = match user.role {
                    access::Role::Admin => "amministratore",
                    access::Role::Reader => "sola lettura",
                };
                println!("{} · {role}", user.name);
            }
        }
        AccessAction::Proxy { trust, base_path } => {
            config::edit(dir, |c| {
                c.access.trusted_proxies = trust.clone();
                c.access.base_path = base_path.clone().filter(|p| !p.is_empty());
                Ok(())
            })?;
            let port = config::load(dir)?.web_port.unwrap_or(0);
            let prefix = base_path.unwrap_or_else(|| "/loglem".into());
            println!(
                "Proxy fidati: {trust:?}. Esempio per il Caddyfile (toglie il prefisso):\n\n\
                 \thandle_path {prefix}/* {{\n\t\treverse_proxy 127.0.0.1:{port}\n\t}}\n\n\
                 Riavvia Loglem per applicare."
            );
        }
        AccessAction::Address { address } => match access::set_address(dir, &address)? {
            None => println!("La pagina risponderà solo su questo computer."),
            Some(ip) => println!(
                "La pagina risponderà su {ip}. Riavvia Loglem per applicare.\n\
                     Attenzione: il collegamento non è cifrato. Usalo dentro una VPN, \
                     oppure dietro un proxy con HTTPS."
            ),
        },
    }
    Ok(())
}

fn alerts_command(dir: &std::path::Path, action: AlertsAction) -> Result<()> {
    use crate::alerts::{Check, Rule};
    let add = |rule: Rule| {
        config::edit(dir, |c| {
            c.alerts.rules.retain(|r| r.name != rule.name);
            c.alerts.rules.push(rule);
            Ok(())
        })
    };
    match action {
        AlertsAction::Monitor { enabled, minutes } => {
            if enabled.is_some() || minutes.is_some() {
                crate::monitor::configure(dir, enabled, minutes)?;
            }
            println!("{}", crate::monitor::line(dir));
            let s = config::load(dir)?.alerts.monitor;
            println!(
                "Impostazioni: {}, ogni {} minuti.",
                if s.enabled { "acceso" } else { "spento" },
                s.minutes
            );
        }
        AlertsAction::EmailSet {
            from,
            to,
            page_url,
            hourly_limit,
        } => {
            let key = match std::env::var("LOGLEM_RESEND_KEY") {
                Ok(key) => key,
                Err(_) => crate::access::read_hidden("Chiave di Resend (non viene mostrata): ")?,
            };
            crate::email::configure(
                dir,
                crate::email::Settings {
                    from,
                    to,
                    page_url,
                    hourly_limit: hourly_limit.unwrap_or(6),
                },
                Some(&key),
            )?;
            println!("Avvisi per mail salvati. Prova con: loglem alerts email-test");
        }
        AlertsAction::EmailTest => {
            crate::email::send_test(dir)?;
            println!("Mail di prova partita. Controlla la casella di posta.");
        }
        AlertsAction::EmailOff => {
            crate::email::disable(dir)?;
            println!("Avvisi per mail spenti e chiave cancellata.");
        }
        AlertsAction::EmailShow => {
            let s = crate::email::status(dir)?;
            if !s.configured {
                println!("Avvisi per mail: non configurati.");
            } else {
                println!(
                    "Avvisi per mail: {}",
                    if s.active {
                        "attivi"
                    } else {
                        "configurati ma senza chiave"
                    }
                );
                println!(
                    "Mittente: {}\nDestinatari: {}\nLimite: {} mail all'ora",
                    s.from,
                    s.to.join(", "),
                    s.hourly_limit
                );
                println!(
                    "Chiave: {}",
                    if s.key_set {
                        "salvata (non si può rileggere)"
                    } else {
                        "mancante"
                    }
                );
                if let Some(last) = s.last {
                    println!(
                        "Ultimo invio: {} · {}{}",
                        last.at,
                        if last.ok { "riuscito" } else { "NON riuscito" },
                        last.error.map(|e| format!(" · {e}")).unwrap_or_default()
                    );
                }
            }
        }
        AlertsAction::List => {
            let config = config::load(dir)?;
            if config.alerts.rules.is_empty() {
                println!("Nessuna regola personale.");
            }
            for rule in &config.alerts.rules {
                println!("regola {}: {}", rule.name, rule.describe());
            }
            match &config.alerts.command {
                Some(command) => println!("Notifica: {command}"),
                None => println!("Nessun comando di notifica."),
            }
            for problem in crate::health::check(dir, crate::health::DEFAULT_MAX_MINUTES, true)? {
                println!("[{}] {}", problem.id, problem.text);
            }
        }
        AlertsAction::AddPattern {
            name,
            text,
            count,
            minutes,
            source,
        } => add(Rule {
            name,
            check: Check::Pattern {
                source,
                text,
                count,
                minutes,
            },
        })?,
        AlertsAction::AddSize {
            name,
            megabytes,
            source,
        } => add(Rule {
            name,
            check: Check::ArchiveSize { source, megabytes },
        })?,
        AlertsAction::AddFreeSpace { name, megabytes } => add(Rule {
            name,
            check: Check::FreeSpace { megabytes },
        })?,
        AlertsAction::Remove { name } => config::edit(dir, |c| {
            let before = c.alerts.rules.len();
            c.alerts.rules.retain(|r| r.name != name);
            if c.alerts.rules.len() == before {
                bail!("La regola {name} non esiste");
            }
            Ok(())
        })?,
        AlertsAction::Command { command } => config::edit(dir, |c| {
            c.alerts.command = Some(command).filter(|c| !c.trim().is_empty());
            Ok(())
        })?,
        AlertsAction::Seen { id } => crate::alerts::acknowledge(dir, &id)?,
    }
    Ok(())
}

/// Chi sta usando il terminale, per il registro degli accessi.
fn terminal_user() -> String {
    format!(
        "terminale:{}",
        std::env::var("USER").unwrap_or_else(|_| "sconosciuto".into())
    )
}

/// Elenca cosa succede ai file originali di una sorgente.
fn print_originals(source: &str, files: &[crate::originals::FileOutcome]) {
    for file in files.iter().filter(|f| f.status != "nothing") {
        let what = match file.status {
            "rewritten" => format!("riscritto, {} righe tolte", file.matched),
            "would_rewrite" => format!("verrebbe riscritto, {} righe da togliere", file.matched),
            _ => format!("saltato: {}", file.note),
        };
        println!("{source}: {} → {what}", file.file);
    }
}
