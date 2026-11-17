use crate::{
    archive::Archive,
    collector, config,
    model::{Input, Source, UpdateSettings},
    service, sources, terminal, timestamps, update, util,
};
use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "loglem",
    version,
    about = "Lightweight Event Manager — terminal log collection and search"
)]
pub struct Cli {
    #[arg(long, env = "LOGLEM_DATA_DIR", global = true)]
    pub data_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Action>,
}

#[derive(Subcommand, Debug)]
pub enum Action {
    /// Read persistent Loglem errors, cause chains, and stack traces.
    Diagnostics,
    /// Raccoglie i log e apre l'interfaccia web (se il servizio è attivo, ne mostra l'indirizzo).
    Web,
    /// Open the keyboard-driven terminal interface (also the default).
    Tui,
    /// Create an empty configuration.
    Init,
    /// Configure sources without opening an interface.
    Source {
        #[command(subcommand)]
        action: SourceAction,
    },
    /// List Podman containers and their pods.
    PodmanList {
        #[arg(long)]
        connection: Option<String>,
    },
    /// Cerca i log di Caddy, Nginx e Apache nelle loro configurazioni.
    Discover,
    /// Elenca i container Docker.
    DockerList {
        #[arg(long)]
        context: Option<String>,
    },
    /// Elenca i pod di Kubernetes in tutti i namespace.
    KubernetesList {
        #[arg(long)]
        context: Option<String>,
    },
    /// Show archive locations, sizes, dates, and collection status.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Collect once and enforce archive retention.
    Collect,
    /// Controlla che Loglem giri e che le sorgenti vengano lette. Esce con codice 1 se c'è
    /// un problema: utilizzabile da cron, systemd o un programma di monitoraggio.
    Check {
        /// Una sorgente senza letture riuscite da più minuti di così è segnalata.
        #[arg(long, default_value_t = crate::health::DEFAULT_MAX_MINUTES)]
        minutes: u64,
    },
    /// Mostra le righe nuove appena arrivano (in locale, o da un altro computer con --server).
    Tail {
        /// Solo questa sorgente.
        #[arg(long)]
        source: Option<String>,
        #[command(flatten)]
        query: crate::query::Query,
        /// Indirizzo della pagina web del server, es. http://10.0.0.1:8790
        #[arg(long, requires = "user")]
        server: Option<String>,
        /// Utente con cui entrare nel server (la password viene chiesta).
        #[arg(long)]
        user: Option<String>,
        /// Quante righe recenti mostrare prima della diretta (solo in locale).
        #[arg(long, default_value_t = 10)]
        history: usize,
    },
    /// Copia coerente di configurazione e archivi in una cartella nuova.
    Backup { target: PathBuf },
    /// Verifica un backup: impronte, integrità SQLite, righe. Esce con codice 1 se non è valido.
    BackupVerify { target: PathBuf },
    /// Ripristina un backup verificato (Loglem deve essere fermo).
    Restore {
        target: PathBuf,
        /// Sostituisce i dati attuali.
        #[arg(long)]
        replace: bool,
    },
    /// Cancella i dati di una persona (GDPR art. 17) da tutte le sorgenti. Senza --apply mostra
    /// solo quante righe toccherebbe. Non tocca i file originali né i backup.
    Erase {
        #[arg(long)]
        source: Option<String>,
        #[command(flatten)]
        query: crate::query::Query,
        /// Perché si cancella, per il registro (es. «richiesta art. 17 del 17/11/2026, pratica 3»).
        #[arg(long)]
        reason: Option<String>,
        /// Cancella anche dai file originali (solo file finiti e già archiviati).
        #[arg(long)]
        originals: bool,
        /// Esegue davvero la cancellazione.
        #[arg(long)]
        apply: bool,
    },
    /// Esporta i dati di una persona (GDPR art. 15) in un file di testo o JSON.
    Export {
        #[arg(long)]
        source: Option<String>,
        #[command(flatten)]
        query: crate::query::Query,
        /// File da creare (solo il proprietario lo legge). Senza, scrive a video.
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Chi ha cercato, esportato o cancellato, e quando (ultimi giorni).
    Audit {
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Statistiche di un log di accesso: richieste per giorno, pagine, codici, IP, lentezza.
    Stats {
        source: String,
        #[arg(long, default_value_t = 7)]
        days: u32,
        #[arg(long)]
        json: bool,
    },
    /// Modalità prova: sigillo giornaliero con impronte concatenate e verifica.
    Proof {
        #[command(subcommand)]
        action: ProofAction,
    },
    /// Dati personali: mascheramento degli IP, campi JSON da tenere, rapporto.
    Privacy {
        #[command(subcommand)]
        action: PrivacyAction,
    },
    /// Allarmi: regole, comando di notifica, «segna come visto».
    Alerts {
        #[command(subcommand)]
        action: AlertsAction,
    },
    /// Accesso alla pagina web da altri computer: utenti e indirizzo.
    Access {
        #[command(subcommand)]
        action: AccessAction,
    },
    /// Servizio: raccoglie i log e serve l'interfaccia web finché non riceve SIGINT o SIGTERM.
    Serve,
    /// Literal, case-sensitive text search; dates are UTC unless a timezone is supplied.
    Search(SearchArgs),
    /// Preview expired entries, or delete them with --apply.
    Prune {
        #[arg(long)]
        apply: bool,
    },
    /// Archive a file explicitly handed off by a producer that has CLOSED it.
    ArchiveCompleted {
        #[arg(long)]
        source: String,
        #[arg(long)]
        file: PathBuf,
        /// Required: the producer will never write to this file again.
        #[arg(long)]
        producer_closed: bool,
        /// Remove the completed original only after byte-for-byte verification.
        #[arg(long)]
        remove: bool,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    Update {
        #[command(subcommand)]
        action: UpdateAction,
    },
    /// Test the executable and embedded SQLite schema without changing user data.
    SelfCheck,
    #[command(name = "__apply-update", hide = true)]
    ApplyUpdate { stage: PathBuf },
}

#[derive(Subcommand, Debug)]
pub enum SourceAction {
    /// Change display name, calendar retention, archive location, or source connection.
    Configure {
        name: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long, requires = "unit")]
        amount: Option<u32>,
        #[arg(long, value_enum, requires = "amount")]
        unit: Option<crate::model::RetentionUnit>,
        #[arg(long, value_enum)]
        log_kind: Option<crate::model::LogKind>,
        #[arg(long)]
        archive_dir: Option<String>,
        #[arg(long)]
        completed_dir: Option<String>,
        #[arg(long)]
        handoff_confirmed: bool,
        /// true: elimina i file ruotati dopo averli archiviati e verificati (es. Caddy).
        #[arg(long)]
        remove_rotated: Option<bool>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        container: Option<String>,
        #[arg(long)]
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
        #[arg(long)]
        name: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 28)]
        days: u32,
    },
    AddFile {
        #[arg(long)]
        name: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 28)]
        days: u32,
        #[arg(long)]
        rotated_pattern: Option<String>,
    },
    /// Journal di systemd, eventualmente solo alcuni servizi o programmi.
    AddJournal {
        #[arg(long)]
        name: String,
        /// Servizio systemd da leggere (ripetibile), es. --unit caddy.service
        #[arg(long = "unit")]
        units: Vec<String>,
        /// Programma syslog da leggere (ripetibile), es. --identifier sshd
        #[arg(long = "identifier")]
        identifiers: Vec<String>,
        /// Campo del journal da confrontare (ripetibile), es. --match CONTAINER_NAME=web
        #[arg(long = "match")]
        matches: Vec<String>,
        /// Un container Podman per nome: continua anche quando viene ricreato.
        #[arg(long)]
        container: Option<String>,
        /// Solo voci fino a questa gravità: 0 emergenza … 3 errore … 7 debug
        #[arg(long)]
        priority: Option<u8>,
        #[arg(long, default_value_t = 28)]
        days: u32,
    },
    /// Log di accesso di Caddy: legge anche i file ruotati e li elimina dopo averli verificati.
    AddCaddy {
        #[arg(long)]
        name: String,
        /// Il file su cui scrive Caddy, es. /var/log/caddy/access.log
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 28)]
        days: u32,
    },
    /// Container Docker (nome o identificativo).
    AddDocker {
        #[arg(long)]
        name: String,
        #[arg(long)]
        container: String,
        #[arg(long)]
        context: Option<String>,
        #[arg(long, default_value_t = 28)]
        days: u32,
    },
    /// Kubernetes: un pod (--pod) oppure i pod con un'etichetta (--selector app=web),
    /// che continua a funzionare quando i pod vengono ricreati.
    AddKubernetes {
        #[arg(long)]
        name: String,
        #[arg(long)]
        namespace: String,
        #[arg(long)]
        pod: Option<String>,
        #[arg(long)]
        selector: Option<String>,
        #[arg(long)]
        container: Option<String>,
        #[arg(long)]
        context: Option<String>,
        #[arg(long, default_value_t = 28)]
        days: u32,
    },
    AddPodman {
        #[arg(long)]
        name: String,
        #[arg(long)]
        container: String,
        #[arg(long, default_value_t = 28)]
        days: u32,
        #[arg(long)]
        connection: Option<String>,
    },
    List,
    Set {
        name: String,
        #[arg(long)]
        days: Option<u32>,
        #[arg(long, action=clap::ArgAction::Set)]
        enabled: Option<bool>,
    },
}

#[derive(Args, Debug, Default)]
pub struct SearchArgs {
    #[arg(long)]
    pub source: Option<String>,
    #[command(flatten)]
    pub query: crate::query::Query,
    #[arg(long)]
    pub from: Option<String>,
    /// YYYY-MM-DD includes that entire day; a full timestamp is an exclusive upper bound.
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long, default_value_t=100, value_parser=clap::value_parser!(u16).range(1..=1000))]
    pub limit: u16,
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
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
#[derive(Subcommand, Debug)]
pub enum ProofAction {
    /// Attiva la modalità prova per una sorgente.
    Enable {
        source: String,
        /// Autorità di marca temporale RFC 3161, es. https://freetsa.org/tsr
        #[arg(long)]
        tsa: Option<String>,
    },
    /// Disattiva la modalità prova (il registro già scritto resta).
    Disable { source: String },
    /// Verifica i giorni sigillati. Con --archive e --chain verifica una copia.
    Verify {
        source: Option<String>,
        #[arg(long, requires = "chain")]
        archive: Option<PathBuf>,
        #[arg(long)]
        chain: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PrivacyAction {
    /// Imposta le tutele di una sorgente. --mask-ips-after-days 0 = subito; vuoto = mai.
    Set {
        source: String,
        #[arg(long)]
        mask_ips_after_days: Option<u32>,
        /// Smette di mascherare gli IP.
        #[arg(long, conflicts_with = "mask_ips_after_days")]
        no_mask: bool,
        /// Campi JSON da tenere, separati da virgola (es. request.uri,status). Vuoto = tutti.
        #[arg(long)]
        keep_fields: Option<String>,
    },
    /// Cosa si conserva, per quanto e con quali tutele (per il registro delle conservazioni).
    Report,
}

#[derive(Subcommand, Debug)]
pub enum AlertsAction {
    /// Elenca regole, comando di notifica e problemi attuali con il loro nome.
    List,
    /// Allarme se un testo compare almeno COUNT volte in MINUTES minuti.
    AddPattern {
        name: String,
        #[arg(long)]
        text: String,
        #[arg(long)]
        count: u32,
        #[arg(long)]
        minutes: u32,
        #[arg(long)]
        source: Option<String>,
    },
    /// Allarme se l'archivio supera MEGABYTES.
    AddSize {
        name: String,
        #[arg(long)]
        megabytes: u64,
        #[arg(long)]
        source: Option<String>,
    },
    /// Allarme se sul disco dei dati restano meno di MEGABYTES liberi.
    AddFreeSpace {
        name: String,
        #[arg(long)]
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

#[derive(Subcommand, Debug)]
pub enum AccessAction {
    /// Crea un utente, o cambia password e ruolo di uno esistente. La password viene chiesta.
    UserAdd {
        name: String,
        #[arg(long, value_enum, default_value = "admin")]
        role: crate::access::Role,
    },
    /// Elimina un utente; le sue sessioni aperte si chiudono subito.
    UserRemove { name: String },
    /// Elenca gli utenti.
    UserList,
    /// Pubblica la pagina dietro un reverse proxy (Caddy, Nginx…). Richiede almeno un utente.
    Proxy {
        /// Indirizzo del proxy da cui accettare X-Forwarded-For/-Proto (ripetibile).
        #[arg(long = "trust")]
        trust: Vec<std::net::IpAddr>,
        /// Prefisso se il proxy non lo toglie, es. /loglem. Vuoto = nessuno.
        #[arg(long)]
        base_path: Option<String>,
    },
    /// Dove ascoltare: «local» (solo questo computer), un IP di questo computer
    /// (es. quello della VPN) o 0.0.0.0. Vale dal prossimo avvio.
    Address { address: String },
}
#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    Install {
        #[arg(long)]
        start: bool,
    },
    Start,
    Stop,
    Status,
}
#[derive(Subcommand, Debug)]
pub enum UpdateAction {
    Configure {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        public_key: String,
    },
    Check,
    Install,
    Status,
}

pub fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, Some(Action::SelfCheck)) {
        let db = rusqlite::Connection::open_in_memory()?;
        db.execute_batch(include_str!("schema.sql"))?;
        println!("Loglem {}: executable and SQLite schema OK", crate::VERSION);
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
        Action::Init => {
            config::edit(dir, |_| Ok(()))?;
            println!("Configuration: {}", dir.join("config.json").display());
            Ok(())
        }
        Action::Source { action } => {
            match action {
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
                    crate::audit::record(dir, "console", "elimina sorgente", &format!("sorgente: {name}"));
                    println!("Sorgente {name} eliminata, con il suo storico.");
                }
                SourceAction::AddFolder { name, path, days } => {
                    let path = fs::canonicalize(path).context("Choose an existing folder")?;
                    if !path.is_dir() {
                        bail!("Choose a folder");
                    }
                    config::add(
                        dir,
                        Source {
                            id: name,
                            retention_days: days,
                            enabled: true,
                            input: Input::Folder { path },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddJournal {
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
                            retention_days: days,
                            input: Input::Journal(filter),
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddCaddy { name, path, days } => {
                    let retention = crate::model::Retention {
                        amount: days,
                        unit: crate::model::RetentionUnit::Days,
                    };
                    let source = crate::management::caddy_source(name, &path, retention)?;
                    let snippet = crate::management::caddy_snippet(&path);
                    config::add(dir, source)?;
                    println!("Sorgente Caddy aggiunta. Nel Caddyfile usa questo blocco log:\n\n{snippet}");
                }
                SourceAction::AddFile {
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
                            retention_days: days,
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
                            retention_days: days,
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
                            retention_days: days,
                            input: Input::Docker {
                                container: found.id,
                                context,
                            },
                            ..Source::default()
                        },
                    )?;
                }
                SourceAction::AddKubernetes {
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
                            retention_days: days,
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
                        source.retention_days = days;
                        source.retention = None;
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
                let cutoff = source.period().cutoff(now);
                let count = if apply {
                    archive.retain_since(cutoff)? as i64
                } else {
                    archive.db.query_row(
                        "SELECT count(*) FROM entries WHERE event_ns<?1 OR collected_ns<?1",
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
                    if fs::metadata(&file)?.len() > 1024 * 1024 {
                        bail!("Configuration file is too large");
                    }
                    let imported = serde_json::from_slice(&fs::read(file)?)?;
                    config::validate(&imported)?;
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
            ServiceAction::Install { start } => service::install(dir, start),
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
