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
    /// Show archive locations, sizes, dates, and collection status.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Collect once and enforce archive retention.
    Collect,
    /// Mostra le righe nuove appena arrivano (in locale, o da un altro computer con --server).
    Tail {
        /// Solo questa sorgente.
        #[arg(long)]
        source: Option<String>,
        /// Solo le righe che contengono questo testo.
        #[arg(long, default_value = "")]
        text: String,
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
    #[arg(long, default_value = "")]
    pub text: String,
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
                    priority,
                    days,
                } => {
                    let filter = crate::sources::journal::Filter {
                        units,
                        identifiers,
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
        Action::Tail {
            source,
            text,
            server,
            user,
            history,
        } => match (server, user) {
            (Some(server), Some(user)) => {
                crate::tail::remote(&server, &user, source.as_deref(), &text)
            }
            _ => crate::tail::local(dir, source.as_deref(), &text, history),
        },
        Action::Access { action } => access_command(dir, action),
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
            let rows = collector::search(
                dir,
                args.source.as_deref(),
                &args.text,
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
            ServiceAction::Start => service::run(&["start", service::UNIT]),
            ServiceAction::Stop => service::run(&["stop", service::UNIT]),
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
