//! Avvisi per mail (facoltativi) tramite Resend.
//!
//! La chiave del servizio sta in un file riservato al proprietario (`secrets/resend.key`):
//! non entra nella configurazione, nelle impostazioni esportate, nei backup, nei log né
//! nei messaggi di errore. Le mail non contengono righe di log.
use crate::{alerts::Change, config, health::Problem, http, timestamps, util};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path, sync::Mutex};

const ENDPOINT: &str = "https://api.resend.com/emails";
const KEY_FILE: &str = "secrets/resend.key";
const STATE_FILE: &str = "email-state.json";
const HOUR_NS: i64 = 3_600_000_000_000;
const DAY_NS: i64 = 24 * HOUR_NS;
const MINUTE_NS: i64 = 60_000_000_000;

/// Un solo invio alla volta: il ciclo di lettura, il controllo e il pulsante di prova.
static LOCK: Mutex<()> = Mutex::new(());

fn six() -> u32 {
    6
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Mittente: `noreply@dominio` oppure `Nome <noreply@dominio>`.
    pub from: String,
    pub to: Vec<String>,
    /// Indirizzo della pagina di Loglem da mettere nella mail (facoltativo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    /// Al massimo quante mail all'ora.
    #[serde(default = "six")]
    pub hourly_limit: u32,
}

fn clean(text: &str) -> bool {
    !text.chars().any(|c| c.is_control())
}

fn valid_address(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    text.len() <= 254
        && !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !text
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "<>,;\"()[]\\".contains(c))
}

fn valid_sender(text: &str) -> bool {
    match text.split_once('<') {
        None => valid_address(text),
        Some((name, rest)) => {
            let name = name.trim();
            rest.strip_suffix('>').is_some_and(valid_address)
                && !name.is_empty()
                && name.len() <= 100
                && clean(name)
                && !name.contains(['<', '>', '"', '@'])
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_sender(self.from.trim()),
            "Il mittente deve essere un indirizzo come noreply@tuodominio.it"
        );
        ensure!(
            (1..=5).contains(&self.to.len()) && self.to.iter().all(|a| valid_address(a)),
            "Indica da 1 a 5 indirizzi validi a cui scrivere"
        );
        ensure!(
            (1..=60).contains(&self.hourly_limit),
            "Il limite va da 1 a 60 mail all'ora"
        );
        if let Some(url) = &self.page_url {
            ensure!(
                (url.starts_with("https://") || url.starts_with("http://"))
                    && url.len() <= 300
                    && !url.chars().any(|c| c.is_whitespace() || c.is_control()),
                "L'indirizzo della pagina deve iniziare con https:// o http://"
            );
        }
        Ok(())
    }
}

// ---------- la chiave ----------

fn key_path(dir: &Path) -> std::path::PathBuf {
    dir.join(KEY_FILE)
}

pub fn has_key(dir: &Path) -> bool {
    key_path(dir).is_file()
}

pub fn save_key(dir: &Path, key: &str) -> Result<()> {
    let key = key.trim();
    ensure!(
        (8..=200).contains(&key.len()) && key.chars().all(|c| c.is_ascii_graphic()),
        "La chiave non sembra valida: copiala intera da Resend, senza spazi"
    );
    util::atomic_write(&key_path(dir), key.as_bytes())
}

pub fn clear_key(dir: &Path) -> Result<()> {
    match fs::remove_file(key_path(dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn read_key(dir: &Path) -> Result<String> {
    let key = fs::read_to_string(key_path(dir)).context("Manca la chiave di Resend")?;
    Ok(key.trim().to_owned())
}

// ---------- stato degli invii ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Last {
    pub at: String,
    pub ok: bool,
    /// `avviso`, `prova` o `riepilogo`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// Quando sono stati fatti gli ultimi tentativi (per il limite all'ora).
    #[serde(default)]
    attempts: Vec<i64>,
    #[serde(default)]
    last: Option<Last>,
    /// Una mail che non è partita (errore o limite raggiunto): si riprova.
    #[serde(default)]
    pending: Option<Message>,
    #[serde(default)]
    failures: u32,
    #[serde(default)]
    retry_at: i64,
    /// Quando è stato ricordato un problema ancora aperto.
    #[serde(default)]
    reminded: BTreeMap<String, i64>,
}

fn load_state(dir: &Path) -> State {
    fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn save_state(dir: &Path, state: &State) -> Result<()> {
    util::atomic_write(&dir.join(STATE_FILE), &serde_json::to_vec_pretty(state)?)
}

/// Quello che la pagina mostra. Non contiene mai la chiave.
#[derive(Debug, Serialize)]
pub struct Status {
    pub configured: bool,
    pub key_set: bool,
    /// Configurata e con la chiave: gli avvisi partono davvero.
    pub active: bool,
    pub from: String,
    pub to: Vec<String>,
    pub page_url: Option<String>,
    pub hourly_limit: u32,
    pub last: Option<Last>,
    pub pending: bool,
}

pub fn status(dir: &Path) -> Result<Status> {
    let settings = config::load(dir)?.alerts.email;
    let state = load_state(dir);
    let key_set = has_key(dir);
    Ok(Status {
        configured: settings.is_some(),
        key_set,
        active: settings.is_some() && key_set,
        from: settings
            .as_ref()
            .map(|s| s.from.clone())
            .unwrap_or_default(),
        to: settings.as_ref().map(|s| s.to.clone()).unwrap_or_default(),
        page_url: settings.as_ref().and_then(|s| s.page_url.clone()),
        hourly_limit: settings.as_ref().map_or(6, |s| s.hourly_limit),
        last: state.last,
        pending: state.pending.is_some(),
    })
}

// ---------- configurazione ----------

/// Salva le impostazioni. Con `key` vuota resta la chiave già salvata.
pub fn configure(dir: &Path, mut settings: Settings, key: Option<&str>) -> Result<()> {
    settings.from = settings.from.trim().to_owned();
    settings.to = settings.to.iter().map(|a| a.trim().to_owned()).collect();
    settings.page_url = settings
        .page_url
        .map(|u| u.trim().to_owned())
        .filter(|u| !u.is_empty());
    settings.validate()?;
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => save_key(dir, key)?,
        None => ensure!(has_key(dir), "Incolla la chiave di Resend"),
    }
    config::edit(dir, |c| {
        c.alerts.email = Some(settings);
        Ok(())
    })
}

/// Spegne gli avvisi per mail e cancella la chiave.
pub fn disable(dir: &Path) -> Result<()> {
    config::edit(dir, |c| {
        c.alerts.email = None;
        Ok(())
    })?;
    clear_key(dir)?;
    let _ = fs::remove_file(dir.join(STATE_FILE));
    Ok(())
}

// ---------- il testo ----------

fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: il buffer è valido per la lunghezza indicata.
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len() - 1) } == 0;
        if ok {
            let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
            let name = String::from_utf8_lossy(&buf[..end]).trim().to_owned();
            if !name.is_empty() {
                return name;
            }
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "server".into())
}

/// Il testo di un problema per la mail: mai dettagli che potrebbero contenere righe di log.
fn mail_text(problem: &Problem) -> String {
    let text = if problem.id.starts_with("errore:") {
        let name = problem
            .text
            .split_once(": lettura non riuscita")
            .map_or("Una sorgente", |(name, _)| name);
        format!("{name}: lettura non riuscita (il dettaglio è nella pagina)")
    } else {
        problem.text.clone()
    };
    text.chars().filter(|c| !c.is_control()).take(240).collect()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn compose(
    host: &str,
    change: &Change,
    reminders: &[(&Problem, i64)],
    link: Option<&str>,
) -> Option<Message> {
    if change.opened.is_empty() && change.closed.is_empty() && reminders.is_empty() {
        return None;
    }
    let subject = if !change.opened.is_empty() {
        format!(
            "Loglem · {host}: {}",
            plural(change.opened.len(), "problema nuovo", "problemi nuovi")
        )
    } else if !change.closed.is_empty() {
        format!(
            "Loglem · {host}: {}",
            plural(change.closed.len(), "problema risolto", "problemi risolti")
        )
    } else {
        format!(
            "Loglem · {host}: {}",
            plural(
                reminders.len(),
                "problema ancora aperto",
                "problemi ancora aperti"
            )
        )
    };
    let mut body = format!("Loglem sul server {host}\n");
    let mut section = |title: &str, lines: Vec<String>| {
        if !lines.is_empty() {
            body.push_str(&format!("\n{title}\n"));
            for line in lines {
                body.push_str(&format!("- {line}\n"));
            }
        }
    };
    section(
        "Nuovi problemi:",
        change.opened.iter().map(mail_text).collect(),
    );
    section("Risolti:", change.closed.iter().map(mail_text).collect());
    section(
        "Ancora aperti da più di 24 ore:",
        reminders
            .iter()
            .map(|(p, hours)| {
                if *hours >= 48 {
                    format!("{} (da {} giorni)", mail_text(p), hours / 24)
                } else {
                    format!("{} (da {hours} ore)", mail_text(p))
                }
            })
            .collect(),
    );
    if let Some(link) = link {
        body.push_str(&format!("\nApri la pagina di Loglem: {link}\n"));
    }
    body.push_str(
        "\nÈ un avviso automatico. Non contiene righe di log. Per non riceverlo più: \
         Sistema → Avvisi per mail → Disattiva.\n",
    );
    Some(Message { subject, body })
}

fn page_link(dir: &Path, settings: &Settings) -> Option<String> {
    if let Some(url) = &settings.page_url {
        return Some(url.clone());
    }
    let cfg = config::load(dir).ok()?;
    let address = cfg.access.address?;
    (!address.is_loopback()).then(|| match (cfg.web_port, address) {
        (Some(port), std::net::IpAddr::V6(a)) => format!("http://[{a}]:{port}"),
        (Some(port), a) => format!("http://{a}:{port}"),
        (None, _) => format!("http://{address}"),
    })
}

// ---------- invio ----------

/// Toglie la chiave da qualunque testo, per sicurezza, e lo accorcia.
fn redact(text: &str, key: &str) -> String {
    let text = if key.is_empty() {
        text.to_owned()
    } else {
        text.replace(key, "***")
    };
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}

fn explain(error: anyhow::Error, reply: &[u8], key: &str) -> anyhow::Error {
    let Some(code) = error.downcast_ref::<http::HttpError>().map(|e| e.0) else {
        return anyhow::anyhow!("{}", redact(&format!("{error:#}"), key));
    };
    let hint = match code {
        401 => "Resend non riconosce la chiave. Controlla di averla copiata intera.",
        403 => "Resend rifiuta l'invio: la chiave non può spedire da questo dominio, oppure il dominio del mittente non è verificato.",
        422 => "Resend non accetta il mittente o il destinatario.",
        429 => "Resend dice che ci sono troppe richieste: riprova tra poco.",
        _ => "Resend ha risposto con un errore.",
    };
    let detail = serde_json::from_slice::<serde_json::Value>(reply)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .map(|m| format!(" (Resend: {})", redact(&m, key)))
        .unwrap_or_default();
    anyhow::anyhow!("{hint} Codice {code}.{detail}")
}

/// L'invio vero: una richiesta a Resend. La chiave viaggia in un file riservato, non negli argomenti.
pub(crate) fn send_to(url: &str, settings: &Settings, key: &str, message: &Message) -> Result<()> {
    let body = serde_json::to_vec(&json!({
        "from": settings.from,
        "to": settings.to,
        "subject": message.subject,
        "text": message.body,
    }))?;
    let authorization = format!("Bearer {key}");
    let mut reply = Vec::new();
    http::transfer(
        url,
        &[
            ("Authorization", authorization.as_str()),
            ("Content-Type", "application/json"),
            ("User-Agent", "loglem"),
        ],
        Some(&body),
        64 * 1024,
        &mut reply,
    )
    .map(|_| ())
    .map_err(|e| explain(e, &reply, key))
}

type Sender<'a> = &'a dyn Fn(&Settings, &str, &Message) -> Result<()>;

fn backoff(failures: u32) -> i64 {
    // 5, 10, 20, 40 minuti, poi ogni ora.
    i64::from(5u32.saturating_mul(1 << failures.min(4)).min(60)) * MINUTE_NS
}

fn recent(state: &State, now: i64) -> usize {
    state
        .attempts
        .iter()
        .filter(|t| now - **t < HOUR_NS)
        .count()
}

/// Le impostazioni e la chiave con cui si spedisce.
struct Account<'a> {
    settings: &'a Settings,
    key: &'a str,
}

fn try_send(
    dir: &Path,
    state: &mut State,
    account: &Account,
    message: Message,
    kind: &str,
    now: i64,
    send: Sender,
) -> Result<()> {
    let (settings, key) = (account.settings, account.key);
    state.attempts.retain(|t| now - *t < HOUR_NS);
    let limit = settings.hourly_limit as usize;
    if recent(state, now) >= limit && kind != "prova" {
        // Troppe mail in un'ora: si manda un riepilogo appena si può.
        let oldest = state.attempts.iter().min().copied().unwrap_or(now);
        state.retry_at = oldest + HOUR_NS;
        state.pending = Some(message);
        state.last = Some(Last {
            at: timestamps::format_ns(now),
            ok: false,
            kind: kind.into(),
            error: Some(format!(
                "Limite di {limit} mail all'ora raggiunto: le notizie partiranno insieme appena possibile."
            )),
        });
        return save_state(dir, state);
    }
    state.attempts.push(now);
    match send(settings, key, &message) {
        Ok(()) => {
            state.failures = 0;
            state.pending = None;
            state.last = Some(Last {
                at: timestamps::format_ns(now),
                ok: true,
                kind: kind.into(),
                error: None,
            });
        }
        Err(error) => {
            let text = redact(&format!("{error:#}"), key);
            if kind != "prova" {
                state.failures += 1;
                state.retry_at = now + backoff(state.failures);
                state.pending = Some(message);
            }
            state.last = Some(Last {
                at: timestamps::format_ns(now),
                ok: false,
                kind: kind.into(),
                error: Some(text),
            });
        }
    }
    save_state(dir, state)
}

/// Dopo ogni controllo: manda una mail se qualcosa è cambiato (o un promemoria dopo 24 ore).
pub(crate) fn notify_with(dir: &Path, change: &Change, now: i64, send: Sender) -> Result<()> {
    let _one = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(settings) = config::load(dir)?.alerts.email else {
        return Ok(());
    };
    let Ok(key) = read_key(dir) else {
        return Ok(());
    };
    let mut state = load_state(dir);
    for closed in &change.closed {
        state.reminded.remove(&closed.id);
    }
    let reminders: Vec<(&Problem, i64)> = change
        .open_long
        .iter()
        .filter(|(p, _)| {
            state
                .reminded
                .get(&p.id)
                .is_none_or(|at| now - *at >= DAY_NS)
        })
        .map(|(p, hours)| (p, *hours))
        .collect();
    let link = page_link(dir, &settings);
    let Some(mut message) = compose(&hostname(), change, &reminders, link.as_deref()) else {
        return save_state(dir, &state);
    };
    if let Some(old) = state.pending.take() {
        message.body.push_str(&format!(
            "\n--- Messaggio precedente, non recapitato ---\n{}",
            old.body.chars().take(4000).collect::<String>()
        ));
    }
    for (problem, _) in &reminders {
        state.reminded.insert(problem.id.clone(), now);
    }
    let account = Account {
        settings: &settings,
        key: &key,
    };
    try_send(dir, &mut state, &account, message, "avviso", now, send)
}

/// Riprova una mail rimasta in coda, se è ora e il limite lo permette.
pub(crate) fn retry_with(dir: &Path, now: i64, send: Sender) -> Result<()> {
    let _one = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut state = load_state(dir);
    let Some(message) = state.pending.clone() else {
        return Ok(());
    };
    if now < state.retry_at {
        return Ok(());
    }
    let Some(settings) = config::load(dir)?.alerts.email else {
        return Ok(());
    };
    let Ok(key) = read_key(dir) else {
        return Ok(());
    };
    state.pending = None;
    let account = Account {
        settings: &settings,
        key: &key,
    };
    try_send(dir, &mut state, &account, message, "riepilogo", now, send)
}

/// La mail di prova: mostra subito se chiave, mittente e dominio vanno bene.
pub(crate) fn test_with(dir: &Path, now: i64, send: Sender) -> Result<()> {
    let _one = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let settings = config::load(dir)?
        .alerts
        .email
        .context("Gli avvisi per mail non sono configurati")?;
    let key = read_key(dir)?;
    let message = Message {
        subject: format!("Loglem · {}: mail di prova", hostname()),
        body: format!(
            "Questa è una mail di prova di Loglem sul server {}.\nSe la leggi, gli avvisi per mail funzionano.\n",
            hostname()
        ),
    };
    let mut state = load_state(dir);
    let account = Account {
        settings: &settings,
        key: &key,
    };
    try_send(dir, &mut state, &account, message, "prova", now, send)?;
    match state.last {
        Some(Last { ok: true, .. }) => Ok(()),
        Some(Last {
            error: Some(error), ..
        }) => bail!("{error}"),
        _ => bail!("Invio non riuscito"),
    }
}

fn production(settings: &Settings, key: &str, message: &Message) -> Result<()> {
    send_to(ENDPOINT, settings, key, message)
}

pub fn send_test(dir: &Path) -> Result<()> {
    test_with(dir, timestamps::now_ns(), &production)
}

/// Manda gli avvisi in un thread a parte: l'invio può durare e non deve fermare la lettura.
pub fn notify(dir: &Path, change: Change) {
    let configured = config::load(dir).is_ok_and(|c| c.alerts.email.is_some());
    if !configured || change.is_empty() {
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        if let Err(error) = notify_with(&dir, &change, timestamps::now_ns(), &production) {
            crate::diagnostics::error("email", "notify", None, &error);
        }
    });
}

/// Chiamata a ogni ciclo di lettura: costa quasi niente se non c'è nulla in coda.
pub fn retry(dir: &Path) {
    let waiting = fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|data| serde_json::from_slice::<State>(&data).ok())
        .is_some_and(|s| s.pending.is_some() && timestamps::now_ns() >= s.retry_at);
    if !waiting {
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        if let Err(error) = retry_with(&dir, timestamps::now_ns(), &production) {
            crate::diagnostics::error("email", "retry", None, &error);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const NOW: i64 = 1_800_000_000_000_000_000;

    fn problem(id: &str, text: &str) -> Problem {
        Problem {
            id: id.into(),
            source: None,
            text: text.into(),
        }
    }

    fn settings() -> Settings {
        Settings {
            from: "Loglem <noreply@mail.example.com>".into(),
            to: vec!["admin@example.com".into()],
            page_url: None,
            hourly_limit: 2,
        }
    }

    fn ready() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        configure(dir.path(), settings(), Some("re_test_key_123456")).unwrap();
        dir
    }

    fn opened(id: &str) -> Change {
        Change {
            opened: vec![problem(
                id,
                &format!("Sorgente {id}: nessuna lettura riuscita da 20 minuti"),
            )],
            ..Change::default()
        }
    }

    #[test]
    fn addresses_and_settings_are_validated() {
        assert!(valid_address("admin@example.com"));
        assert!(!valid_address("admin@example"));
        assert!(!valid_address("a b@example.com"));
        assert!(!valid_address("a@b@example.com"));
        assert!(!valid_address("x@example.com\nBcc: y@z.com"));
        assert!(valid_sender("noreply@example.com"));
        assert!(valid_sender("Loglem <noreply@example.com>"));
        assert!(!valid_sender("Loglem <noreply@example.com"));
        assert!(!valid_sender("Lo\nglem <noreply@example.com>"));
        let mut bad = settings();
        bad.to = vec![];
        assert!(bad.validate().is_err());
        bad = settings();
        bad.hourly_limit = 0;
        assert!(bad.validate().is_err());
        bad = settings();
        bad.page_url = Some("javascript:alert(1)".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn the_key_lives_in_a_private_file_and_never_in_the_configuration_or_status() {
        let dir = ready();
        let config_text = fs::read_to_string(dir.path().join("config.json")).unwrap();
        assert!(!config_text.contains("re_test_key"));
        let status = serde_json::to_string(&status(dir.path()).unwrap()).unwrap();
        assert!(!status.contains("re_test_key"));
        assert!(status.contains("\"key_set\":true"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(key_path(dir.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "il file della chiave deve essere privato");
        }
        // Senza chiave nuova resta quella salvata; senza nessuna chiave si rifiuta.
        configure(dir.path(), settings(), None).unwrap();
        disable(dir.path()).unwrap();
        assert!(!has_key(dir.path()));
        assert!(configure(dir.path(), settings(), None).is_err());
    }

    #[test]
    fn the_message_names_the_problem_but_hides_error_details() {
        let change = Change {
            opened: vec![
                problem(
                    "ferma:web",
                    "Negozio: nessuna lettura riuscita da 20 minuti",
                ),
                problem(
                    "errore:web",
                    "Negozio: lettura non riuscita: password=hunter2 in /srv/x",
                ),
            ],
            closed: vec![problem(
                "ferma:api",
                "API: nessuna lettura riuscita da 30 minuti",
            )],
            ..Change::default()
        };
        let m = compose("server1", &change, &[], Some("https://logs.example.com")).unwrap();
        assert_eq!(m.subject, "Loglem · server1: 2 problemi nuovi");
        assert!(m.body.contains("Nuovi problemi:"));
        assert!(m.body.contains("Risolti:"));
        assert!(m.body.contains("https://logs.example.com"));
        assert!(!m.body.contains("hunter2"));
        assert!(m
            .body
            .contains("Negozio: lettura non riuscita (il dettaglio è nella pagina)"));
        assert!(compose("s", &Change::default(), &[], None).is_none());
    }

    #[test]
    fn one_mail_per_change_and_a_reminder_only_once_a_day() {
        let dir = ready();
        let sent = RefCell::new(Vec::<Message>::new());
        let send = |_: &Settings, _: &str, m: &Message| {
            sent.borrow_mut().push(m.clone());
            Ok(())
        };
        notify_with(dir.path(), &opened("ferma:web"), NOW, &send).unwrap();
        assert_eq!(sent.borrow().len(), 1);
        // Nessun cambiamento: nessuna mail.
        notify_with(dir.path(), &Change::default(), NOW + MINUTE_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 1);
        // Dopo 30 ore ancora aperto: un promemoria. Poi più niente per 24 ore.
        let long = Change {
            open_long: vec![(problem("ferma:web", "Negozio: ferma"), 30)],
            ..Change::default()
        };
        notify_with(dir.path(), &long, NOW + 30 * HOUR_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 2);
        assert!(sent.borrow()[1].body.contains("da 30 ore"));
        notify_with(dir.path(), &long, NOW + 31 * HOUR_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 2);
        notify_with(dir.path(), &long, NOW + 55 * HOUR_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 3);
    }

    #[test]
    fn a_failed_send_is_kept_and_retried_with_waiting_time() {
        let dir = ready();
        let fail = |_: &Settings, _: &str, _: &Message| -> Result<()> { bail!("rete assente") };
        notify_with(dir.path(), &opened("ferma:web"), NOW, &fail).unwrap();
        let s = status(dir.path()).unwrap();
        assert!(s.pending);
        assert_eq!(s.last.as_ref().map(|l| l.ok), Some(false));
        // Troppo presto: non riprova.
        let sent = RefCell::new(0);
        let ok = |_: &Settings, _: &str, _: &Message| {
            *sent.borrow_mut() += 1;
            Ok(())
        };
        retry_with(dir.path(), NOW + MINUTE_NS, &ok).unwrap();
        assert_eq!(*sent.borrow(), 0);
        // Dopo l'attesa riprova e svuota la coda.
        retry_with(dir.path(), NOW + 11 * MINUTE_NS, &ok).unwrap();
        assert_eq!(*sent.borrow(), 1);
        let s = status(dir.path()).unwrap();
        assert!(!s.pending);
        assert_eq!(s.last.map(|l| l.ok), Some(true));
    }

    #[test]
    fn the_hourly_limit_holds_mails_back_and_sends_them_together_later() {
        let dir = ready(); // limite: 2 all'ora
        let sent = RefCell::new(Vec::<Message>::new());
        let send = |_: &Settings, _: &str, m: &Message| {
            sent.borrow_mut().push(m.clone());
            Ok(())
        };
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            notify_with(dir.path(), &opened(id), NOW + i as i64 * MINUTE_NS, &send).unwrap();
        }
        assert_eq!(sent.borrow().len(), 2, "la terza mail è trattenuta");
        assert!(status(dir.path()).unwrap().pending);
        retry_with(dir.path(), NOW + 10 * MINUTE_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 2, "il limite vale ancora");
        retry_with(dir.path(), NOW + 61 * MINUTE_NS, &send).unwrap();
        assert_eq!(sent.borrow().len(), 3);
        assert!(!status(dir.path()).unwrap().pending);
    }

    #[test]
    fn errors_never_show_the_key_and_explain_what_to_do() {
        let key = "re_test_key_123456";
        let err = explain(
            http::HttpError(403).into(),
            br#"{"statusCode":403,"message":"The domain is not verified for re_test_key_123456","name":"validation_error"}"#,
            key,
        );
        let text = format!("{err:#}");
        assert!(!text.contains(key));
        assert!(text.contains("dominio del mittente non è verificato"));
        assert!(text.contains("Codice 403"));
        let other = explain(
            anyhow::anyhow!("curl: Bearer re_test_key_123456 rifiutato"),
            b"",
            key,
        );
        assert!(!format!("{other:#}").contains(key));
    }

    #[test]
    fn a_test_mail_reports_the_real_error() {
        let dir = ready();
        let fail = |_: &Settings, _: &str, _: &Message| -> Result<()> { bail!("Codice 401") };
        let err = test_with(dir.path(), NOW, &fail).unwrap_err();
        assert!(format!("{err:#}").contains("401"));
        // La prova non mette niente in coda.
        assert!(!status(dir.path()).unwrap().pending);
        let ok = |_: &Settings, _: &str, _: &Message| Ok(());
        test_with(dir.path(), NOW + MINUTE_NS, &ok).unwrap();
    }

    /// Un finto server locale: controlla l'intestazione, il corpo e che la chiave non sia negli argomenti.
    #[test]
    fn the_real_request_carries_the_key_in_a_header_and_the_message_as_json() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                seen.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"id\":\"x\"}\n")
                .unwrap();
            String::from_utf8_lossy(&seen).to_string()
        });
        let message = Message {
            subject: "Prova".into(),
            body: "Corpo".into(),
        };
        send_to(
            &format!("http://{address}/emails"),
            &settings(),
            "re_test_key_123456",
            &message,
        )
        .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /emails"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer re_test_key_123456"));
        let body = request.split_once("\r\n\r\n").unwrap().1;
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["from"], "Loglem <noreply@mail.example.com>");
        assert_eq!(json["to"][0], "admin@example.com");
        assert_eq!(json["subject"], "Prova");
        assert_eq!(json["text"], "Corpo");
    }
}
