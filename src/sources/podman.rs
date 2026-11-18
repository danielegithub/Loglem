//! Container Podman e Docker: `logs --timestamps` letto a istantanee.
//!
//! Le righe identiche con la stessa data vengono riconosciute tra un'istantanea e
//! l'altra, quindi rileggere un intervallo non le duplica. Lo stesso schema serve
//! anche a Kubernetes (`read_snapshot`).
use crate::{archive::Archive, model::Entry, process, timestamps, util, BATCH_SIZE};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{process::Command, process::ExitStatus, time::Duration};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    #[default]
    Podman,
    Docker,
}

impl Runtime {
    pub fn name(self) -> &'static str {
        match self {
            Runtime::Podman => "Podman",
            Runtime::Docker => "Docker",
        }
    }

    pub fn command(self, connection: Option<&str>) -> Command {
        let (program, option) = match self {
            Runtime::Podman => ("podman", "--connection"),
            Runtime::Docker => ("docker", "--context"),
        };
        let mut command = Command::new(program);
        if let Some(connection) = connection {
            command.args([option, connection]);
        }
        command
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub pod: String,
    pub state: String,
    /// Immagine (es. `docker.io/library/nginx:1.27`): serve a riconoscere il programma.
    pub image: String,
    /// Progetto compose (`com.docker.compose.project` o `io.podman.compose.project`).
    pub project: String,
}

/// Un container di Podman o di Docker, con il runtime che lo mostra.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Listed {
    pub runtime: Runtime,
    pub id: String,
    pub name: String,
    pub image: String,
    pub project: String,
    pub state: String,
}

/// Tutti i container visibili, di Podman e di Docker insieme. Un runtime che manca o non
/// risponde viene saltato.
pub fn list_all() -> Vec<Listed> {
    let mut all = Vec::new();
    for runtime in [Runtime::Podman, Runtime::Docker] {
        if let Ok(list) = discover_with(runtime, None) {
            all.extend(list.into_iter().map(|c| Listed {
                runtime,
                id: c.id,
                name: c.name,
                image: c.image,
                project: c.project,
                state: c.state,
            }));
        }
    }
    all
}

pub fn command(connection: Option<&str>) -> Command {
    Runtime::Podman.command(connection)
}

pub fn discover(connection: Option<&str>) -> Result<Vec<Container>> {
    discover_with(Runtime::Podman, connection)
}

/// Elenco dei container. Podman restituisce un array JSON, Docker un oggetto per riga.
pub fn discover_with(runtime: Runtime, connection: Option<&str>) -> Result<Vec<Container>> {
    let mut command = runtime.command(connection);
    match runtime {
        Runtime::Podman => command.args(["ps", "--all", "--pod", "--format=json"]),
        Runtime::Docker => command.args(["ps", "--all", "--no-trunc", "--format", "{{json .}}"]),
    };
    let data = process::output(&mut command, Duration::from_secs(20)).with_context(|| {
        format!(
            "{} non risponde: è installato e in funzione?",
            runtime.name()
        )
    })?;
    let text = String::from_utf8_lossy(&data);
    let rows: Vec<serde_json::Value> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text)?
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?
    };
    rows.iter()
        .map(|row| container_from_json(row, runtime))
        .collect()
}

fn container_from_json(row: &serde_json::Value, runtime: Runtime) -> Result<Container> {
    // Un campo può essere testo o elenco di testi (Podman: "Names": ["web"]).
    let field = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|key| {
                let value = row.get(key)?;
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_array()?.first()?.as_str().map(str::to_owned))
            })
            .unwrap_or_default()
    };
    let id = field(&["Id", "ID", "id"]);
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!(
            "{} ha restituito un identificativo di container non valido",
            runtime.name()
        );
    }
    // Podman: «Labels» è un oggetto; Docker: un testo «chiave=valore,chiave=valore».
    let label = |key: &str| -> Option<String> {
        match row.get("Labels")? {
            serde_json::Value::Object(map) => map.get(key)?.as_str().map(str::to_owned),
            serde_json::Value::String(text) => text
                .split(',')
                .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
                .map(str::to_owned),
            _ => None,
        }
    };
    Ok(Container {
        id,
        name: field(&["Names", "Name", "names"]),
        pod: field(&["PodName", "Pod", "pod"]),
        state: field(&["State", "Status", "state"]),
        image: field(&["Image", "image"]),
        project: label("com.docker.compose.project")
            .or_else(|| label("io.podman.compose.project"))
            .unwrap_or_default(),
    })
}

/// Una riga `<data RFC3339> <messaggio>`, con un eventuale prefisso `[pod/nome/container] `
/// (Kubernetes con più pod): il prefisso resta nel messaggio.
fn decode(bytes: Vec<u8>, stderr: bool, store: &Archive, now: i64) -> Result<Entry> {
    let line = std::str::from_utf8(&bytes).context("Output non UTF-8")?;
    let (prefix, rest) = match line.strip_prefix('[').and_then(|l| l.split_once("] ")) {
        Some((prefix, rest)) => (format!("[{prefix}] "), rest),
        None => (String::new(), line),
    };
    let (stamp, message) = rest.split_once(' ').context("Riga di log senza data")?;
    let event_ns = chrono::DateTime::parse_from_rfc3339(stamp)
        .context("Diagnostica o riga senza data RFC3339")?
        .timestamp_nanos_opt()
        .context("Data fuori intervallo")?;
    let message = format!("{prefix}{}", message.trim_end_matches(['\r', '\n']));
    let hash = util::digest(format!("{stderr}\0{event_ns}\0{message}").as_bytes());
    let occurrence = store.occurrence(&hash)?;
    Ok(Entry {
        key: format!("podman:{hash}:{occurrence}"),
        file_id: None,
        generation: 0,
        offset: 0,
        event_ns,
        collected_ns: now,
        timestamp_origin: "event",
        message,
    })
}

/// Esito di un'istantanea: righe aggiunte, codice di uscita e righe non riconosciute.
pub struct Snapshot {
    pub added: usize,
    pub status: ExitStatus,
    pub invalid: usize,
    pub diagnostic: String,
}

/// Esegue il comando dei log e salva le righe datate a partire da `earliest`.
/// `scope` distingue le chiavi di sorgenti diverse nello stesso archivio.
pub fn read_snapshot(
    store: &mut Archive,
    command: &mut Command,
    scope: &str,
    now: i64,
    earliest: i64,
) -> Result<Snapshot> {
    store.begin_podman_snapshot()?;
    let mut batch = Vec::new();
    let mut batch_bytes = 0;
    let mut added = 0;
    let mut invalid = 0;
    let mut diagnostic = String::new();
    let status = process::stream(command, Duration::from_secs(120), |stderr, bytes| {
        let original = String::from_utf8_lossy(&bytes)
            .trim()
            .chars()
            .take(2048)
            .collect::<String>();
        match decode(bytes, stderr, store, now) {
            Ok(mut entry) => {
                entry.key = format!("{scope}:{}", entry.key);
                if entry.event_ns >= earliest {
                    batch_bytes += entry.message.len();
                    batch.push(entry);
                    if batch.len() >= BATCH_SIZE || batch_bytes >= 2 * 1024 * 1024 {
                        added += store.save_entries(&batch)?;
                        batch.clear();
                        batch_bytes = 0;
                    }
                }
            }
            Err(error) => {
                // Anche i messaggi di errore del programma arrivano su stderr, insieme
                // all'stderr del container: si conserva l'originale, non solo l'errore.
                invalid += 1;
                if diagnostic.len() < 8192 {
                    diagnostic.push_str(&format!("{original} ({error:#})\n"));
                }
            }
        }
        Ok(())
    })?;
    added += store.save_entries(&batch)?;
    Ok(Snapshot {
        added,
        status,
        invalid,
        diagnostic,
    })
}

/// Da dove riprendere: il punto salvato meno un secondo (le righe ripetute si
/// riconoscono), ma mai prima dell'inizio della conservazione.
pub fn resume_point(store: &Archive, checkpoint: &str, earliest: i64) -> Result<i64> {
    Ok(store
        .get_state(checkpoint)?
        .and_then(|s| s.parse::<i64>().ok())
        .map(|v| v.saturating_sub(1_000_000_000))
        .unwrap_or(earliest)
        .max(earliest))
}

pub fn collect(
    store: &mut Archive,
    runtime: Runtime,
    container: &str,
    connection: Option<&str>,
    now: i64,
    earliest: i64,
) -> Result<usize> {
    migrate_legacy(store, container)?;
    remember_name(store, runtime, container, connection);
    let checkpoint = format!("podman_until:{container}");
    let since = resume_point(store, &checkpoint, earliest)?;
    let mut command = runtime.command(connection);
    command.args([
        "logs",
        "--timestamps",
        "--since",
        &timestamps::format_ns(since),
        "--until",
        &timestamps::format_ns(now),
        container,
    ]);
    let snapshot = read_snapshot(store, &mut command, container, now, earliest)?;
    if !snapshot.status.success() {
        if let Some(message) = missing_container(store, runtime, container, connection, since, now)?
        {
            bail!("{message}");
        }
    }
    if !snapshot.status.success() || snapshot.invalid != 0 {
        bail!(
            "Lettura {} non completata (uscita: {}; righe non riconosciute: {}). {} \
             I log già salvati restano disponibili; il punto di lettura non è avanzato",
            runtime.name(),
            snapshot.status,
            snapshot.invalid,
            snapshot.diagnostic.trim_end()
        );
    }
    // Il punto avanza solo dopo un'istantanea completa e riuscita.
    store.set_state(&checkpoint, &now.to_string())?;
    Ok(snapshot.added)
}

/// Dove scrive i log un container e con quale limite.
#[derive(Debug, Clone, Serialize)]
pub struct LogConfig {
    pub driver: String,
    pub max_size: Option<String>,
    /// Consiglio per limitare la copia dei log tenuta dal runtime.
    pub advice: String,
    /// Il container scrive nel journal: si può seguire per nome tramite il journal.
    pub journald: bool,
}

pub fn log_config(
    runtime: Runtime,
    connection: Option<&str>,
    container: &str,
) -> Result<LogConfig> {
    let data = process::output(
        runtime
            .command(connection)
            .args(["inspect", "--", container]),
        Duration::from_secs(20),
    )?;
    let value: serde_json::Value = serde_json::from_slice(&data)?;
    let config = &value[0]["HostConfig"]["LogConfig"];
    let driver = config["Type"].as_str().unwrap_or("sconosciuto").to_owned();
    // Podman: "Size": "10MB" o "-1B" (nessun limite); Docker: Config.max-size.
    let max_size = config["Size"]
        .as_str()
        .or_else(|| config["Config"]["max-size"].as_str())
        .filter(|s| !s.starts_with('-') && *s != "0B")
        .map(str::to_owned);
    // Seguirlo dal journal richiede di poterlo leggere qui: serve `journalctl` su questo
    // computer e un container locale (non una connessione remota, né la macchina virtuale
    // di Podman su macOS).
    let uses_journal = driver == "journald";
    let journald = uses_journal
        && connection.is_none()
        && crate::sources::journal::available()
        && match runtime {
            // Anche un contesto predefinito può puntare a un host remoto.
            Runtime::Podman => {
                cfg!(target_os = "linux")
                    && process::output(
                        Command::new("podman").args([
                            "--remote=false",
                            "container",
                            "exists",
                            "--",
                            container,
                        ]),
                        Duration::from_secs(3),
                    )
                    .is_ok()
            }
            Runtime::Docker => {
                // Verifica l'origine nel journal locale prima di proporre di seguirla per nome.
                let field = format!("CONTAINER_ID_FULL={container}");
                process::output(
                    Command::new("journalctl").args([
                        "-n",
                        "1",
                        "--output=json",
                        "--no-pager",
                        &field,
                    ]),
                    Duration::from_secs(3),
                )
                .is_ok_and(|data| {
                    serde_json::from_slice::<serde_json::Value>(&data)
                        .is_ok_and(|v| v["CONTAINER_ID_FULL"] == container)
                })
            }
        };
    let advice = match (driver.as_str(), &max_size) {
        ("journald", _) if !journald => {
            "I log del container vanno nel journal, che da qui non si può leggere direttamente: \
            Loglem li legge con il comando del runtime. La durata del journal la imposta \
            l'amministratore di quel sistema (MaxRetentionSec o SystemMaxUse)."
                .to_owned()
        }
        ("journald", _) => {
            "I log del container vanno nel journal, condiviso con tutto il sistema. \
            Per limitarne la durata un amministratore imposta MaxRetentionSec o SystemMaxUse in \
            journald.conf: vale per tutti i programmi, e Loglem non cancella mai voci dal \
            journal. Consiglio: segui il container per nome tramite il journal, così non perdi \
            righe quando viene ricreato."
                .to_owned()
        }
        ("none", _) => "Il driver dei log è disattivato per questo container.".into(),
        (_, Some(size)) => format!(
            "{} indica un limite di dimensione di {size} per la rotazione: le righe più \
             vecchie spariscono. Se Loglem resta fermo a lungo, quelle non lette si perdono.",
            runtime.name()
        ),
        (_, None) => format!(
            "{} non indica un limite di dimensione per questo container.",
            runtime.name()
        ),
    };
    Ok(LogConfig {
        driver,
        max_size,
        advice,
        journald,
    })
}

const NAME: &str = "podman_name";

/// Ricorda il nome del container: se sparisce, serve a capire se è stato ricreato.
fn remember_name(store: &Archive, runtime: Runtime, container: &str, connection: Option<&str>) {
    if store.get_state(NAME).ok().flatten().is_some() {
        return;
    }
    if let Ok(found) = discover_with(runtime, connection) {
        if let Some(c) = found.iter().find(|c| c.id == container) {
            let _ = store.set_state(NAME, &c.name);
        }
    }
}

/// Se il container non esiste più, registra la lacuna e restituisce un messaggio chiaro.
/// Le righe scritte dopo l'ultima lettura erano solo nel container: sono perse.
fn missing_container(
    store: &Archive,
    runtime: Runtime,
    container: &str,
    connection: Option<&str>,
    since: i64,
    now: i64,
) -> Result<Option<String>> {
    let Ok(found) = discover_with(runtime, connection) else {
        return Ok(None); // Il programma stesso non risponde: non è il container a mancare.
    };
    if found.iter().any(|c| c.id == container) {
        return Ok(None);
    }
    let name = store.get_state(NAME)?.unwrap_or_default();
    let recreated = !name.is_empty() && found.iter().any(|c| c.name == name);
    let mut message = format!(
        "Il container {} non esiste più: le righe scritte dopo l'ultima lettura sono perse",
        if name.is_empty() {
            &container[..12.min(container.len())]
        } else {
            &name
        }
    );
    if recreated {
        message.push_str(
            ". Un container con lo stesso nome è stato ricreato: collegalo nelle impostazioni \
             della sorgente («Container ricreato?»), oppure segui il container per nome \
             tramite il journal, che non perde righe ai nuovi rilasci",
        );
    }
    store.record_gap(since, now, &message)?;
    Ok(Some(message))
}

pub fn migrate_legacy(store: &Archive, container: &str) -> Result<()> {
    if store.get_state("podman_scoped_keys")?.is_none() {
        crate::blocks::unpack(
            &store.db,
            "id IN (SELECT block_id FROM block_keys WHERE key LIKE 'podman:%')",
            &[],
        )?;
        store.db.execute(
            "UPDATE entries SET record_key = ?1 || ':' || record_key \
             WHERE record_key LIKE 'podman:%'",
            [container],
        )?;
        if let Some(until) = store.get_state("podman_until")? {
            store.set_state(&format!("podman_until:{container}"), &until)?;
        }
        store.set_state("podman_scoped_keys", "1")?;
    }
    Ok(())
}
