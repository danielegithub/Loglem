//! Small local HTTP interface. Bind port 0: the OS reserves a free port atomically.
use crate::{access, audit, collector, config, query::Query, sources, timestamps, util};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const MAX_REQUEST: usize = 64 * 1024;

struct App {
    dir: PathBuf,
    worker: crate::runtime::Worker,
    sessions: access::Sessions,
    /// Usato quando non ci sono utenti: la pagina lo riceve e lo rimanda in ogni richiesta.
    local_token: String,
}

impl App {
    fn new(dir: PathBuf) -> Result<Self> {
        let random = util::random_bytes()?;
        Ok(Self {
            worker: crate::runtime::Worker::start(dir.clone()),
            dir,
            sessions: access::Sessions::default(),
            local_token: util::digest(&random),
        })
    }
}

/// Il servizio di Loglem: raccoglie i log e serve l'interfaccia web nello stesso
/// processo. Una sola istanza per cartella dati; le altre mostrano il suo indirizzo.
pub fn serve(dir: &Path) -> Result<()> {
    util::private_dir(dir)?;
    let dir = fs::canonicalize(dir)?;
    let Ok(_instance) = util::Lock::acquire(&dir.join("service.lock")) else {
        return already_running(&dir);
    };
    let config = config::load(&dir)?;
    migrate_archives(&dir, &config);
    let access = config.access;
    let listener = bind(&dir, access.bind_address())?;
    if check_previous_stop(&dir)? {
        println!(
            "Nota: l'ultimo arresto non è stato regolare. Riprendo dall'ultimo punto salvato."
        );
    }
    let address = listener.local_addr()?.to_string();
    let who = if access.login_required() {
        "Serve nome utente e password (loglem access user-list)."
    } else {
        "Accesso solo da questo computer."
    };
    println!(
        "Loglem · raccolta dei log e interfaccia web\n\
         Apri nel browser: {}\n\
         Dati in: {}\n\
         {who} Ctrl+C per fermare.",
        page_url(&address),
        dir.display()
    );

    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, stop.clone())?;
    }
    // Non bloccante, per poter controllare la richiesta di arresto.
    listener.set_nonblocking(true)?;
    let app = Arc::new(App::new(dir)?);
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Relaxed) {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_nonblocking(false)?;
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            let _ = respond(
                stream,
                503,
                "application/json",
                b"{\"error\":\"Server occupato, riprova tra poco\"}",
            );
            continue;
        }
        let (app, address, active) = (app.clone(), address.clone(), active.clone());
        std::thread::spawn(move || {
            if let Err(error) = handle(stream, &app, &address) {
                crate::diagnostics::error("http", "handle_request", None, &error);
            }
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
    println!("Arresto: ultima lettura di tutte le sorgenti (al massimo 30 secondi)…");
    if !app.worker.finish(Duration::from_secs(30)) {
        println!("Ultima lettura non completata in tempo: riprenderà al prossimo avvio.");
    }
    fs::remove_file(app.dir.join(RUNNING))?;
    Ok(())
}

/// Esiste mentre Loglem è in funzione; se c'è ancora all'avvio, l'arresto precedente
/// è stato brusco (crash, corrente, kill -9).
const RUNNING: &str = "running";

/// Segnala un arresto brusco precedente. Le letture sono salvate insieme ai loro punti
/// di ripartenza, quindi si riprende da dove ci si era fermati senza perdere righe.
fn check_previous_stop(dir: &Path) -> Result<bool> {
    let marker = dir.join(RUNNING);
    let unclean = marker.exists();
    if unclean {
        crate::diagnostics::error(
            "service",
            "startup",
            None,
            &anyhow::anyhow!(
                "Loglem non si era fermato in modo regolare (crash, mancanza di corrente o \
                 arresto forzato). La lettura riprende dall'ultimo punto salvato."
            ),
        );
    }
    util::atomic_write(&marker, std::process::id().to_string().as_bytes())?;
    Ok(unclean)
}

/// Le richieste della diretta restano aperte fino a 15 secondi: servono più posti.
const MAX_CONNECTIONS: usize = 16;

/// Indirizzo da aprire nel browser; per «tutte le interfacce» si suggerisce questo computer.
fn page_url(address: &str) -> String {
    match address.strip_prefix("0.0.0.0:") {
        Some(port) => format!("http://<indirizzo di questo computer>:{port} (tutte le interfacce)"),
        None => format!("http://{address}"),
    }
}

/// Porta gli archivi del formato precedente al nuovo, prima di accettare richieste.
fn migrate_archives(dir: &Path, config: &crate::model::Config) {
    for source in &config.sources {
        let path = config::archive_path(dir, &source.id);
        if !path.exists() || !crate::archive::Archive::needs_migration(&path) {
            continue;
        }
        println!("Aggiorno l'archivio «{}» al nuovo formato…", source.id);
        if let Err(error) = crate::archive::Archive::open(&path) {
            // Non si ferma tutto per un archivio: la sorgente mostrerà l'errore.
            crate::diagnostics::error("service", "migrate_archive", Some(&source.id), &error);
            println!("  non riuscito: {error:#}");
        }
    }
}

/// Usa la porta salvata in configurazione; al primo avvio ne chiede una libera
/// al sistema e la salva, così l'indirizzo resta uguale dopo un riavvio.
fn bind(dir: &Path, host: std::net::IpAddr) -> Result<TcpListener> {
    if let Some(port) = config::load(dir)?.web_port {
        return TcpListener::bind((host, port)).with_context(|| {
            format!(
                "Non riesco ad ascoltare su {host}:{port}: la porta è usata da un altro \
                 programma o l'indirizzo non appartiene a questo computer. \
                 Porta: loglem config web-port <numero>; indirizzo: loglem access address"
            )
        });
    }
    let listener = TcpListener::bind((host, 0))?;
    let port = listener.local_addr()?.port();
    config::edit(dir, |c| {
        c.web_port = Some(port);
        Ok(())
    })?;
    Ok(listener)
}

fn already_running(dir: &Path) -> Result<()> {
    let config = config::load(dir)?;
    match config.web_port {
        Some(port) => {
            let address = format!("{}:{port}", config.access.bind_address());
            println!(
                "Loglem è già in esecuzione (ad esempio come servizio).\n\
                 Apri nel browser: {}",
                page_url(&address)
            );
            Ok(())
        }
        None => bail!("Un'altra istanza di Loglem sta usando {}", dir.display()),
    }
}

fn respond(stream: TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    respond_with(stream, status, content_type, "", body)
}

/// Risposta HTTP completa; `extra` contiene intestazioni aggiuntive già terminate da `\r\n`.
fn respond_with(
    mut stream: TcpStream,
    status: u16,
    content_type: &str,
    extra: &str,
    body: &[u8],
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        301 => "Moved Permanently",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Service Unavailable",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'self'; script-src 'self'; \
         style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; \
         form-action 'self'\r\n\
         {extra}\r\n",
        body.len(),
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn json_error(stream: TcpStream, status: u16, message: &str, extra: Value) -> Result<()> {
    let mut body = json!({ "error": message });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }
    respond(
        stream,
        status,
        "application/json",
        &serde_json::to_vec(&body)?,
    )
}

struct Request {
    method: String,
    route: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

/// Legge una richiesta HTTP/1.1 con limiti su riga, intestazioni e corpo.
/// `Err(text)` = richiesta da rifiutare con quel messaggio.
fn read_request(stream: &TcpStream) -> Result<std::result::Result<Request, &'static str>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.by_ref().take(4097).read_line(&mut first)?;
    let fields = first.split_whitespace().collect::<Vec<_>>();
    if first.len() > 4096 || fields.len() != 3 || fields[2] != "HTTP/1.1" {
        return Ok(Err("Invalid request"));
    }
    let mut headers = HashMap::new();
    let mut total = first.len();
    loop {
        let mut line = String::new();
        let n = reader.by_ref().take(8193).read_line(&mut line)?;
        total += n;
        if n == 0 || n > 8192 || total > 16384 {
            return Ok(Err("Invalid headers"));
        }
        if line == "\r\n" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Ok(Err("Invalid header"));
        };
        if headers
            .insert(key.to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            return Ok(Err("Duplicate header"));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Ok(Err("Unsupported body encoding"));
    }
    let mut body = Vec::new();
    if fields[0] == "POST" {
        let length = headers
            .get("content-length")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        if length > MAX_REQUEST {
            return Ok(Err("Request too large"));
        }
        body = vec![0; length];
        reader.read_exact(&mut body)?;
    }
    Ok(Ok(Request {
        method: fields[0].to_owned(),
        route: fields[1].to_owned(),
        headers,
        body,
    }))
}

/// Pagine e file statici. Con gli utenti attivi, la pagina principale richiede l'accesso.
fn serve_static(stream: TcpStream, route: &str, csrf: Option<&str>) -> Result<()> {
    let (content_type, body): (&str, std::borrow::Cow<[u8]>) = match (route, csrf) {
        ("/", Some(token)) => (
            "text/html; charset=utf-8",
            include_str!("../web/index.html")
                .replace("__LOGLEM_TOKEN__", token)
                .into_bytes()
                .into(),
        ),
        ("/", None) => (
            "text/html; charset=utf-8",
            include_bytes!("../web/login.html").as_slice().into(),
        ),
        ("/app.js", _) => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/app.js").as_slice().into(),
        ),
        ("/login.js", _) => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/login.js").as_slice().into(),
        ),
        ("/style.css", _) => (
            "text/css; charset=utf-8",
            include_bytes!("../web/style.css").as_slice().into(),
        ),
        _ => return respond(stream, 404, "text/plain", b"Not found"),
    };
    respond(stream, 200, content_type, &body)
}

/// Dati di una persona: anteprima, cancellazione, esportazione e registro degli accessi.
/// Solo per gli amministratori (non sono tra le azioni dei lettori).
/// Suggerimenti su cosa c'è nel log e su come trattarlo (solo amministratori).
/// Le scelte finiscono nel registro degli accessi da `advice`.
fn advice_route(route: &str, app: &App, body: &Value, who: &str) -> Result<Value> {
    let name = text(body, "name")?;
    let who = format!("web:{who}");
    match route {
        "/api/advice" => crate::advice::report(&app.dir, name, crate::advice::DEFAULT_LANGUAGE),
        "/api/advice-apply" => {
            crate::advice::apply(&app.dir, name, text(body, "id")?, &who)?;
            app.worker.request();
            Ok(json!({"message": "Proposta applicata."}))
        }
        _ => {
            crate::advice::choose(
                &app.dir,
                name,
                text(body, "id")?,
                text(body, "choice")?,
                &who,
            )?;
            Ok(json!({"message": "Scelta registrata."}))
        }
    }
}

fn data_rights(route: &str, app: &App, body: &Value, who: &str) -> Result<Value> {
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let query: Query = serde_json::from_value(body.clone())?;
    let originals = body
        .get("originals")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match route {
        "/api/erase-preview" => {
            let counts: Vec<Value> = crate::erase::preview(&app.dir, source, &query)?
                .into_iter()
                .map(|(name, rows)| json!({"source": name, "rows": rows}))
                .collect();
            let files = if originals {
                crate::erase::preview_originals(&app.dir, source, &query)?
                    .into_iter()
                    .map(|(name, files)| json!({"source": name, "files": files}))
                    .collect()
            } else {
                Vec::new()
            };
            Ok(json!({"counts": counts, "originals": files}))
        }
        "/api/erase" => {
            let reason = body.get("reason").and_then(Value::as_str).unwrap_or("");
            let done = crate::erase::erase(
                &app.dir,
                source,
                &query,
                reason,
                &format!("web:{who}"),
                originals,
            )?;
            app.worker.request();
            let files: Vec<Value> = done
                .originals
                .iter()
                .map(|(name, files)| json!({"source": name, "files": files}))
                .collect();
            Ok(json!({
                "rows": done.rows,
                "sources": done.sources,
                "resealed_days": done.resealed_days,
                "originals": files,
            }))
        }
        "/api/export" => {
            let mut out = Vec::new();
            let rows = crate::erase::export(
                &app.dir,
                source,
                &query,
                &mut out,
                false,
                &format!("web:{who}"),
            )?;
            if out.len() > 20 * 1024 * 1024 {
                bail!("Troppe righe per la pagina ({rows}): restringi il filtro o usa «loglem export»");
            }
            Ok(json!({"rows": rows, "text": String::from_utf8_lossy(&out)}))
        }
        _ => Ok(json!({"entries": audit::recent(&app.dir, 90)?})),
    }
}

/// Le azioni che finiscono nel registro degli accessi, con cosa scriverci (mai i valori
/// cercati: solo il tipo di filtro).
fn audited(route: &str, body: &Value) -> Option<(&'static str, String)> {
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("tutte");
    let filters = || {
        serde_json::from_value::<Query>(body.clone())
            .map(|q| audit::filter_kinds(&q))
            .unwrap_or_default()
    };
    match route {
        "/api/search" => Some((
            "ricerca",
            format!("sorgente: {source}; filtri: {}", filters()),
        )),
        "/api/tail" if body.get("after").is_none_or(Value::is_null) => Some((
            "diretta",
            format!("sorgente: {source}; filtri: {}", filters()),
        )),
        "/api/stats" => Some(("statistiche", format!("sorgente: {source}"))),
        "/api/config-export" => Some(("esporta impostazioni", String::new())),
        "/api/config-import" => Some(("importa impostazioni", String::new())),
        "/api/update-install" => Some(("aggiornamento", String::new())),
        "/api/add" => Some((
            "nuova sorgente",
            format!(
                "tipo: {}",
                body.get("kind").and_then(Value::as_str).unwrap_or("?")
            ),
        )),
        "/api/source-remove" => Some((
            "elimina sorgente",
            format!(
                "sorgente: {}",
                body.get("name").and_then(Value::as_str).unwrap_or("?")
            ),
        )),
        "/api/source-settings" | "/api/retention" => Some((
            "modifica impostazioni",
            format!(
                "sorgente: {}",
                body.get("name").and_then(Value::as_str).unwrap_or("?")
            ),
        )),
        _ => None,
    }
}

/// Azioni concesse anche a chi può solo consultare.
const READER_ROUTES: &[&str] = &[
    "/api/status",
    "/api/search",
    "/api/tail",
    "/api/stats",
    "/api/diagnostics",
    "/api/me",
    "/api/logout",
    "/api/alert-seen",
];

fn handle(stream: TcpStream, app: &App, address: &str) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let request = match read_request(&stream)? {
        Ok(request) => request,
        Err(message) => return respond(stream, 400, "text/plain", message.as_bytes()),
    };
    let access = config::load(&app.dir)?.access;
    let login = access.login_required();
    let host = request.headers.get("host").cloned().unwrap_or_default();
    let client = access.client(
        stream.peer_addr()?.ip(),
        request.headers.get("x-forwarded-for").map(String::as_str),
        request.headers.get("x-forwarded-proto").map(String::as_str),
    );
    // Dietro un proxy che non toglie il prefisso (es. /loglem), lo si toglie qui.
    let base = access.base_path.as_deref().unwrap_or("");
    let path = request.route.split('?').next().unwrap_or("");
    if !base.is_empty() && path == base {
        let location = format!("Location: {base}/\r\n");
        return respond_with(stream, 301, "text/plain", &location, b"");
    }
    let route = path
        .strip_prefix(base)
        .filter(|r| r.starts_with('/'))
        .unwrap_or(path)
        .to_owned();
    // Path=/: il proxy può pubblicare la pagina sotto prefissi diversi (/loglem, /lg…).
    let cookie_attributes = format!(
        "HttpOnly; SameSite=Strict; Path=/{}",
        if client.https { "; Secure" } else { "" }
    );
    // Senza utenti la pagina vale solo per l'indirizzo esatto (difesa dal DNS rebinding);
    // con gli utenti protegge la password, e il nome usato per raggiungerla è libero.
    if !login && host != address {
        return respond(
            stream,
            403,
            "text/plain",
            b"Use the exact local address shown by Loglem",
        );
    }
    let session_id = access::session_cookie(request.headers.get("cookie")).map(str::to_owned);
    let session = session_id
        .as_deref()
        .and_then(|id| app.sessions.get(&access, id));

    if request.method == "GET" {
        let csrf = match (login, &session) {
            (false, _) => Some(app.local_token.as_str()),
            (true, Some(session)) => Some(session.csrf.as_str()),
            (true, None) => None,
        };
        return serve_static(stream, &route, csrf);
    }
    if request.method != "POST"
        || request.headers.get("content-type").map(String::as_str) != Some("application/json")
    {
        return respond(stream, 403, "text/plain", b"Local session required");
    }
    if let Some(origin) = request.headers.get("origin") {
        let allowed = if login {
            *origin == format!("http://{host}") || *origin == format!("https://{host}")
        } else {
            *origin == format!("http://{address}")
        };
        if !allowed {
            return respond(stream, 403, "text/plain", b"Origin refused");
        }
    }
    let body = match serde_json::from_slice::<Value>(&request.body) {
        Ok(body) => body,
        Err(_) => return json_error(stream, 400, "Richiesta non valida", json!({})),
    };
    let route = route.as_str();
    let token = request.headers.get("x-loglem-token").map(String::as_str);

    if route == "/api/login" {
        if !login {
            return json_error(
                stream,
                400,
                "Non ci sono utenti: l'accesso non serve",
                json!({}),
            );
        }
        let name = body.get("user").and_then(Value::as_str).unwrap_or("");
        let password = body.get("password").and_then(Value::as_str).unwrap_or("");
        let attempt = app.sessions.login(&access, client.ip, name, password);
        match &attempt {
            Ok(_) => audit::record(&app.dir, name, "accesso", &format!("da {}", client.ip)),
            Err(_) => audit::record(
                &app.dir,
                name,
                "accesso fallito",
                &format!("da {}", client.ip),
            ),
        }
        return match attempt {
            Ok((id, session)) => {
                let cookie = format!(
                    "Set-Cookie: {}={id}; {cookie_attributes}; Max-Age=43200\r\n",
                    access::COOKIE
                );
                let body =
                    json!({"token": session.csrf, "user": session.user, "role": session.role});
                respond_with(
                    stream,
                    200,
                    "application/json",
                    &cookie,
                    &serde_json::to_vec(&body)?,
                )
            }
            Err(error) => json_error(stream, 401, &error.to_string(), json!({})),
        };
    }

    // Chi chiama, e che cosa può fare.
    let role = if login {
        let Some(session) = &session else {
            return json_error(
                stream,
                401,
                "Accesso scaduto: entra di nuovo",
                json!({"login": true}),
            );
        };
        if token != Some(session.csrf.as_str()) {
            return respond(stream, 403, "text/plain", b"Session token required");
        }
        Some(session.role)
    } else {
        if token != Some(app.local_token.as_str()) {
            return respond(stream, 403, "text/plain", b"Local session required");
        }
        None
    };
    if role == Some(access::Role::Reader) && !READER_ROUTES.contains(&route) {
        return json_error(
            stream,
            403,
            "Il tuo utente può solo consultare i log",
            json!({}),
        );
    }

    let who = session
        .as_ref()
        .map_or_else(|| "locale".to_owned(), |s| s.user.clone());
    if let Some((action, detail)) = audited(route, &body) {
        audit::record(&app.dir, &who, action, &detail);
    }
    let result = match route {
        "/api/me" => Ok(json!({
            "login": login,
            "user": session.as_ref().map(|s| s.user.clone()),
            "role": role.unwrap_or(access::Role::Admin),
        })),
        "/api/erase-preview" | "/api/erase" | "/api/export" | "/api/audit" => {
            data_rights(route, app, &body, &who)
        }
        "/api/advice" | "/api/advice-apply" | "/api/advice-choice" => {
            advice_route(route, app, &body, &who)
        }
        "/api/logout" => {
            if let Some(id) = &session_id {
                app.sessions.logout(id);
            }
            let expired = format!(
                "Set-Cookie: {}=; {cookie_attributes}; Max-Age=0\r\n",
                access::COOKIE
            );
            return respond_with(stream, 200, "application/json", &expired, b"{}");
        }
        _ => api(route, app, body),
    };
    match result {
        Ok(value) => respond(
            stream,
            200,
            "application/json",
            &serde_json::to_vec(&value)?,
        ),
        Err(error) => {
            crate::diagnostics::error("web", route, None, &error);
            json_error(
                stream,
                400,
                &format!("{error:#}"),
                json!({"diagnostics": "diagnostics/loglem.log"}),
            )
        }
    }
}

fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing {key}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewSource {
    kind: String,
    name: String,
    path: Option<PathBuf>,
    container: Option<String>,
    connection: Option<String>,
    days: Option<u32>,
    retention: Option<crate::model::Retention>,
    #[serde(default)]
    log_kind: crate::model::LogKind,
    archive_dir: Option<String>,
    #[serde(default)]
    journal: Option<sources::journal::Filter>,
    #[serde(default)]
    kubernetes: Option<sources::kubernetes::Target>,
}

fn api(route: &str, app: &App, body: Value) -> Result<Value> {
    let dir = &app.dir;
    match route {
        "/api/status" => {
            let sources = collector::statuses(dir)?;
            let mut problems =
                crate::health::problems(&sources, crate::health::DEFAULT_MAX_MINUTES);
            problems.extend(crate::alerts::evaluate(dir)?);
            let problems = crate::alerts::hide_acknowledged(dir, problems);
            Ok(json!({
                "sources": sources,
                "archive_directory": dir,
                "version": crate::VERSION,
                "collection": app.worker.progress(),
                "problems": problems,
            }))
        }
        "/api/folders" => {
            let requested = body.get("path").and_then(Value::as_str).unwrap_or("");
            let path = if requested.is_empty() {
                std::env::current_dir()?
            } else {
                fs::canonicalize(requested)
                    .context("Cartella non trovata. Controlla il percorso.")?
            };
            let mut folders = Vec::new();
            for entry in fs::read_dir(&path).context("Non hai accesso a questa cartella")? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    folders.push(entry.path());
                }
                if folders.len() > 2000 {
                    bail!("Troppi elementi: scrivi il percorso di una sottocartella");
                }
            }
            folders.sort();
            let logs = sources::file::folder_files(&path)?;
            Ok(json!({"path":path,"parent":path.parent(),"folders":folders,"logs":logs}))
        }
        "/api/podman" => {
            let connection = body
                .get("connection")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let runtime = match body.get("runtime").and_then(Value::as_str) {
                Some("docker") => sources::podman::Runtime::Docker,
                _ => sources::podman::Runtime::Podman,
            };
            let containers = sources::podman::discover_with(runtime, connection)?;
            Ok(json!({ "containers": containers }))
        }
        "/api/container-logs" => {
            let runtime = match body.get("runtime").and_then(Value::as_str) {
                Some("docker") => sources::podman::Runtime::Docker,
                _ => sources::podman::Runtime::Podman,
            };
            let connection = body
                .get("connection")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            Ok(serde_json::to_value(sources::podman::log_config(
                runtime,
                connection,
                text(&body, "container")?,
            )?)?)
        }
        "/api/kubernetes-pods" => {
            let context = body
                .get("context")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            Ok(json!({ "pods": sources::kubernetes::discover(context)? }))
        }
        "/api/add" => {
            let request: NewSource = serde_json::from_value(body)?;
            if request.kind == "caddy" {
                return add_caddy(app, request);
            }
            if request.kind == "journal" {
                return add_journal(app, request);
            }
            let input = match request.kind.as_str() {
                "folder" => {
                    let path = fs::canonicalize(request.path.context("Scegli una cartella")?)?;
                    if !path.is_dir() {
                        bail!("Scegli una cartella");
                    }
                    crate::model::Input::Folder { path }
                }
                "podman" => {
                    let id = request.container.context("Scegli un container")?;
                    let found = sources::podman::discover(request.connection.as_deref())?
                        .into_iter()
                        .find(|c| c.id == id || c.name == id)
                        .context("Container non trovato")?;
                    crate::model::Input::Podman {
                        container: found.id,
                        connection: request.connection,
                    }
                }
                "docker" => {
                    let id = request.container.context("Scegli un container")?;
                    let runtime = sources::podman::Runtime::Docker;
                    let found =
                        sources::podman::discover_with(runtime, request.connection.as_deref())?
                            .into_iter()
                            .find(|c| c.id == id || c.name == id)
                            .context("Container non trovato")?;
                    crate::model::Input::Docker {
                        container: found.id,
                        context: request.connection,
                    }
                }
                "file" => {
                    // Un log di Nginx o Apache: anche i file ruotati da logrotate
                    // (access.log.1, access.log.2.gz…).
                    let path = fs::canonicalize(request.path.context("Scegli un file")?)?;
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .context("Nome non valido")?;
                    crate::model::Input::File {
                        rotated_pattern: Some(format!("{}.*", glob::Pattern::escape(name))),
                        path,
                    }
                }
                "kubernetes" => {
                    let target = request.kubernetes.context("Scegli un pod")?;
                    target.validate()?;
                    crate::model::Input::Kubernetes(target)
                }
                _ => bail!("Tipo sorgente sconosciuto"),
            };
            let archive_dir = match request.archive_dir.filter(|s| !s.trim().is_empty()) {
                Some(path) => {
                    let path = PathBuf::from(path);
                    if !path.is_absolute() {
                        bail!("La cartella SQLite deve essere assoluta");
                    }
                    crate::util::private_dir(&path)?;
                    Some(fs::canonicalize(path)?)
                }
                None => None,
            };
            let source = crate::model::Source {
                id: request.name,
                input,
                retention: request.retention,
                retention_days: request.days.unwrap_or(28),
                log_kind: request.log_kind,
                archive_dir,
                ..Default::default()
            };
            let mut candidate = crate::config::load(dir)?;
            candidate.sources.push(source.clone());
            crate::config::validate(&candidate)?;
            if source.archive_path(dir).exists() {
                bail!("Esiste già un archivio con questo identificatore; scegli un nome diverso");
            }
            // A new source has no existing archive to relocate. Concurrent
            // collection of other sources must not prevent adding it.
            let _access = crate::util::DataAccess::read(dir)?;
            let parent = source.archive_path(dir).parent().unwrap().to_owned();
            crate::util::private_dir(&parent)?;
            let probe = parent.join(format!(".loglem-probe-{}", timestamps::now_ns()));
            crate::util::private_file(&probe)?.sync_all()?;
            fs::remove_file(probe)?;
            crate::config::add(dir, source)?;
            app.worker.request();
            Ok(json!({"message":"Sorgente salvata. Lettura automatica avviata.","collecting":true}))
        }
        "/api/source-settings" => {
            let change: crate::management::Change = serde_json::from_value(body)?;
            let source = crate::management::apply(dir, change)?;
            app.worker.request();
            Ok(
                json!({"source":source,"message":"Impostazioni salvate; archivio e storico disponibili."}),
            )
        }
        "/api/source-remove" => {
            let name = text(&body, "name")?.to_owned();
            crate::management::remove(dir, &name)?;
            app.worker.request();
            Ok(json!({"message":"Sorgente e storico eliminati."}))
        }
        "/api/retention" => {
            let name = text(&body, "name")?.to_owned();
            let period = match body.get("retention") {
                Some(value) => serde_json::from_value(value.clone())?,
                None => crate::model::Retention {
                    amount: body
                        .get("days")
                        .and_then(Value::as_u64)
                        .filter(|n| *n <= 36500)
                        .context("Indica la durata")? as u32,
                    unit: crate::model::RetentionUnit::Days,
                },
            };
            crate::management::apply(
                dir,
                crate::management::Change {
                    name,
                    retention: Some(period),
                    ..Default::default()
                },
            )?;
            app.worker.request();
            Ok(json!({"message":"Durata salvata. Verrà applicata alla prossima raccolta."}))
        }
        "/api/tail" => tail(app, &body),
        "/api/discover" => {
            // Solo quello che non è già collegato.
            let config = config::load(dir)?;
            let used: Vec<&Path> = config
                .sources
                .iter()
                .filter_map(|s| match &s.input {
                    crate::model::Input::File { path, .. } => Some(path.as_path()),
                    _ => None,
                })
                .collect();
            let found: Vec<_> = crate::discover::find_all()
                .into_iter()
                .filter(|f| f.exists && !used.iter().any(|u| *u == f.path))
                .collect();
            Ok(json!({ "found": found }))
        }
        "/api/update-check" => {
            let manifest = crate::update::check(dir)?;
            Ok(json!({"installed": crate::VERSION, "latest": manifest.version}))
        }
        "/api/update-install" => {
            // Scaricare può richiedere tempo: si lavora in background e la pagina legge lo stato.
            let dir = dir.clone();
            util::atomic_write(
                &dir.join("updates/status.txt"),
                b"Scaricamento della nuova versione...",
            )?;
            std::thread::spawn(move || {
                let result = crate::update::prepare(&dir)
                    .and_then(|stage| crate::update::launch(&dir, &stage));
                if let Err(error) = result {
                    let text = format!("Aggiornamento non riuscito: {error:#}");
                    let _ = util::atomic_write(&dir.join("updates/status.txt"), text.as_bytes());
                    crate::diagnostics::error("update", "web_install", None, &error);
                }
            });
            Ok(json!({"message": "Aggiornamento avviato"}))
        }
        "/api/update-status" => Ok(json!({
            "status": fs::read_to_string(dir.join("updates/status.txt"))
                .unwrap_or_else(|_| "Nessun aggiornamento richiesto.".into()),
            "version": crate::VERSION,
        })),
        "/api/config-export" => {
            // Senza utenti e indirizzo: un file scaricato non deve portare con sé le credenziali.
            let mut exported = config::load(dir)?;
            exported.access = Default::default();
            exported.web_port = None;
            Ok(serde_json::to_value(exported)?)
        }
        "/api/config-import" => {
            let mut imported: crate::model::Config = serde_json::from_value(
                body.get("config")
                    .cloned()
                    .context("Manca la configurazione")?,
            )?;
            config::edit(dir, |current| {
                // Utenti, indirizzo e porta restano quelli di questo server.
                imported.access = current.access.clone();
                imported.web_port = current.web_port;
                config::validate(&imported)?;
                *current = imported;
                Ok(())
            })?;
            app.worker.request();
            Ok(json!({"message": "Impostazioni importate."}))
        }
        "/api/stats" => {
            let days = body
                .get("days")
                .and_then(Value::as_u64)
                .unwrap_or(7)
                .clamp(1, 3650) as u32;
            Ok(serde_json::to_value(crate::stats::compute(
                dir,
                text(&body, "source")?,
                days,
            )?)?)
        }
        "/api/alert-seen" => {
            crate::alerts::acknowledge(dir, text(&body, "id")?)?;
            Ok(json!({}))
        }
        "/api/journal-units" => Ok(json!({ "units": sources::journal::units()? })),
        "/api/caddy-snippet" => {
            let path = PathBuf::from(text(&body, "path")?);
            Ok(json!({ "snippet": crate::management::caddy_snippet(&path) }))
        }
        "/api/collect" => {
            app.worker.request();
            Ok(
                json!({"message":"Lettura richiesta. I risultati appariranno automaticamente.","collecting":true}),
            )
        }
        "/api/diagnostics" => crate::diagnostics::recent(dir),
        "/api/search" => {
            let parse = |key: &str, end: bool, default: i64| -> Result<i64> {
                match body
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    Some(value) => timestamps::parse_date(value, end),
                    None => Ok(default),
                }
            };
            let rows = collector::search(
                dir,
                body.get("source")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty()),
                &serde_json::from_value::<Query>(body.clone())?,
                parse("from", false, i64::MIN)?,
                parse("to", true, i64::MAX)?,
                200,
            )?;
            Ok(json!({"rows":rows}))
        }
        _ => bail!("Unknown action"),
    }
}

/// Diretta: aspetta fino a 15 secondi che arrivino righe nuove, poi risponde.
/// Senza `after` restituisce solo i punti di partenza.
fn tail(app: &App, body: &Value) -> Result<Value> {
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let query: Query = serde_json::from_value(body.clone())?;
    let after: Option<HashMap<String, i64>> = match body.get("after") {
        Some(Value::Null) | None => None,
        Some(value) => Some(serde_json::from_value(value.clone())?),
    };
    let started = std::time::Instant::now();
    loop {
        let (rows, next) = collector::tail(&app.dir, source, &query, after.as_ref())?;
        if !rows.is_empty() || after.is_none() || started.elapsed() > Duration::from_secs(15) {
            return Ok(json!({ "rows": rows, "after": next }));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Aggiunge il journal di systemd, con i filtri scelti nella pagina.
fn add_journal(app: &App, request: NewSource) -> Result<Value> {
    let source = crate::model::Source {
        id: request.name,
        input: crate::model::Input::Journal(request.journal.unwrap_or_default()),
        retention: request.retention,
        retention_days: request.days.unwrap_or(28),
        log_kind: request.log_kind,
        ..Default::default()
    };
    let _access = util::DataAccess::read(&app.dir)?;
    config::add(&app.dir, source)?;
    app.worker.request();
    Ok(json!({"message": "Journal collegato. Lettura automatica avviata.", "collecting": true}))
}

/// Aggiunge il log di accesso di Caddy con la pulizia dei file ruotati.
fn add_caddy(app: &App, request: NewSource) -> Result<Value> {
    let path = request.path.context("Scegli il file .log di Caddy")?;
    let retention = request.retention.unwrap_or(crate::model::Retention {
        amount: request.days.unwrap_or(28),
        unit: crate::model::RetentionUnit::Days,
    });
    let source = crate::management::caddy_source(request.name, &path, retention)?;
    let snippet = crate::management::caddy_snippet(&path);
    let _access = util::DataAccess::read(&app.dir)?;
    config::add(&app.dir, source)?;
    app.worker.request();
    Ok(json!({
        "message": "Sorgente Caddy salvata. Lettura automatica avviata.",
        "collecting": true,
        "snippet": snippet,
    }))
}
