//! Accesso alla pagina web da altri computer: utenti, password e sessioni.
//!
//! Senza utenti la pagina risponde solo su questo computer (127.0.0.1). Con almeno un
//! utente si può scegliere un altro indirizzo (per esempio quello della VPN) e ogni
//! accesso richiede nome e password. Le password sono salvate solo come impronta Argon2.
use anyhow::{bail, Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Può fare tutto: aggiungere sorgenti, cambiare impostazioni.
    Admin,
    /// Può solo consultare: sorgenti, ricerca, diretta.
    Reader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub name: String,
    pub role: Role,
    /// Impronta Argon2id in formato PHC; la password non è mai salvata.
    pub password_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    /// Indirizzo su cui ascoltare; assente = solo questo computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<IpAddr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<User>,
    /// Reverse proxy fidati: solo da loro si accettano X-Forwarded-For e X-Forwarded-Proto.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_proxies: Vec<IpAddr>,
    /// Percorso sotto cui il proxy pubblica la pagina senza toglierlo, es. `/loglem`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

impl Access {
    pub fn is_default(&self) -> bool {
        self.address.is_none()
            && self.users.is_empty()
            && self.trusted_proxies.is_empty()
            && self.base_path.is_none()
    }

    /// Con almeno un utente la pagina chiede sempre nome e password.
    pub fn login_required(&self) -> bool {
        !self.users.is_empty()
    }

    pub fn bind_address(&self) -> IpAddr {
        self.address
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
    }

    pub fn validate(&self) -> Result<()> {
        if !self.bind_address().is_loopback() && self.users.is_empty() {
            bail!(
                "Per aprire la pagina ad altri computer serve almeno un utente con password: \
                 loglem access user-add NOME"
            );
        }
        if !self.trusted_proxies.is_empty() && self.users.is_empty() {
            bail!("Dietro un reverse proxy la pagina va protetta: crea prima un utente");
        }
        if let Some(path) = &self.base_path {
            if !path.starts_with('/') || path.ends_with('/') || path.contains(['?', '#', ' ']) {
                bail!("Il percorso base deve essere del tipo /loglem");
            }
        }
        let mut names = std::collections::HashSet::new();
        for user in &self.users {
            validate_name(&user.name)?;
            if !names.insert(&user.name) {
                bail!("Utente {} ripetuto", user.name);
            }
        }
        Ok(())
    }

    pub fn user(&self, name: &str) -> Option<&User> {
        self.users.iter().find(|u| u.name == name)
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        bail!("Il nome utente può contenere da 1 a 64 lettere, numeri, '-', '_' o '.'");
    }
    Ok(())
}

pub const MIN_PASSWORD: usize = 12;

/// Parametri Argon2id raccomandati da OWASP con poca memoria (7 MiB, 5 passaggi).
fn hasher() -> Argon2<'static> {
    let params = Params::new(7 * 1024, 5, 1, None).expect("parametri Argon2 validi");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(password: &str) -> Result<String> {
    if password.chars().count() < MIN_PASSWORD {
        bail!("La password deve avere almeno {MIN_PASSWORD} caratteri");
    }
    let salt = new_salt()?;
    Ok(hasher()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Impronta della password non calcolabile: {e}"))?
        .to_string())
}

use crate::util::random_bytes;

fn new_salt() -> Result<SaltString> {
    SaltString::encode_b64(&random_bytes()?[..16])
        .map_err(|e| anyhow::anyhow!("Sale non generabile: {e}"))
}

fn password_matches(user: &User, password: &str) -> bool {
    PasswordHash::new(&user.password_hash)
        .map(|hash| hasher().verify_password(password.as_bytes(), &hash).is_ok())
        .unwrap_or(false)
}

/// Cosa si sa della richiesta dopo aver considerato un eventuale proxy fidato.
pub struct Client {
    /// L'indirizzo reale del visitatore (per il blocco dei tentativi).
    pub ip: IpAddr,
    /// Vero se il visitatore usa HTTPS verso il proxy.
    pub https: bool,
}

impl Access {
    /// Le intestazioni del proxy valgono solo se la connessione arriva da un proxy fidato.
    pub fn client(&self, peer: IpAddr, forwarded_for: Option<&str>, proto: Option<&str>) -> Client {
        if !self.trusted_proxies.contains(&peer) {
            return Client {
                ip: peer,
                https: false,
            };
        }
        // L'ultimo indirizzo è quello aggiunto dal proxy fidato: gli altri li manda il client.
        let ip = forwarded_for
            .and_then(|list| list.rsplit(',').next())
            .and_then(|ip| ip.trim().parse().ok())
            .unwrap_or(peer);
        Client {
            ip,
            https: proto.is_some_and(|p| p.eq_ignore_ascii_case("https")),
        }
    }
}

// ---------- gestione da terminale ----------

/// Crea un utente, oppure cambia password e ruolo di uno esistente.
pub fn save_user(dir: &std::path::Path, name: &str, role: Role, password: &str) -> Result<bool> {
    validate_name(name)?;
    let password_hash = hash_password(password)?;
    let mut created = false;
    crate::config::edit(dir, |config| {
        let users = &mut config.access.users;
        match users.iter_mut().find(|u| u.name == name) {
            Some(user) => {
                user.role = role;
                user.password_hash = password_hash;
            }
            None => {
                users.push(User {
                    name: name.to_owned(),
                    role,
                    password_hash,
                });
                created = true;
            }
        }
        Ok(())
    })?;
    Ok(created)
}

pub fn remove_user(dir: &std::path::Path, name: &str) -> Result<()> {
    crate::config::edit(dir, |config| {
        let before = config.access.users.len();
        config.access.users.retain(|u| u.name != name);
        if config.access.users.len() == before {
            bail!("L'utente {name} non esiste");
        }
        Ok(())
    })
}

/// `local` = solo questo computer; altrimenti un indirizzo IP di questo computer
/// (per esempio quello della VPN) oppure `0.0.0.0` per tutte le interfacce.
pub fn set_address(dir: &std::path::Path, value: &str) -> Result<Option<IpAddr>> {
    let address = match value.trim() {
        "local" | "localhost" | "127.0.0.1" => None,
        other => Some(
            other
                .parse::<IpAddr>()
                .with_context(|| format!("{other} non è un indirizzo IP"))?,
        ),
    };
    crate::config::edit(dir, |config| {
        config.access.address = address;
        Ok(())
    })?;
    Ok(address)
}

// ---------- sessioni ----------

const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct Session {
    pub user: String,
    pub role: Role,
    /// Da inviare in ogni richiesta (`X-Loglem-Token`): protegge dalle richieste
    /// fatte da altri siti con il cookie del browser.
    pub csrf: String,
    expires: Instant,
}

#[derive(Default)]
pub struct Sessions {
    open: Mutex<HashMap<String, Session>>,
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

fn random_token() -> Result<String> {
    Ok(crate::util::digest(&random_bytes()?))
}

impl Sessions {
    /// Controlla nome e password; restituisce l'identificativo della sessione.
    pub fn login(
        &self,
        access: &Access,
        from: IpAddr,
        name: &str,
        password: &str,
    ) -> Result<(String, Session)> {
        {
            let failures = self.failures.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((count, since)) = failures.get(&from) {
                if *count >= MAX_FAILURES && since.elapsed() < LOCKOUT {
                    let minutes = (LOCKOUT - since.elapsed()).as_secs() / 60 + 1;
                    bail!("Troppi tentativi sbagliati. Riprova tra {minutes} minuti.");
                }
            }
        }
        let user = access.user(name);
        // Il controllo costa uguale anche per utenti inesistenti: non rivela chi esiste.
        let valid = match user {
            Some(user) => password_matches(user, password),
            None => {
                if let Ok(salt) = new_salt() {
                    let _ = hasher().hash_password(password.as_bytes(), &salt);
                }
                false
            }
        };
        let Some(user) = user.filter(|_| valid) else {
            let mut failures = self.failures.lock().unwrap_or_else(|p| p.into_inner());
            let entry = failures.entry(from).or_insert((0, Instant::now()));
            if entry.1.elapsed() >= LOCKOUT {
                *entry = (0, Instant::now());
            }
            entry.0 += 1;
            bail!("Nome utente o password non corretti");
        };
        self.failures
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&from);
        let session = Session {
            user: user.name.clone(),
            role: user.role,
            csrf: random_token()?,
            expires: Instant::now() + SESSION_LIFETIME,
        };
        let id = random_token()?;
        let mut open = self.open.lock().unwrap_or_else(|p| p.into_inner());
        open.retain(|_, s| s.expires > Instant::now());
        open.insert(id.clone(), session.clone());
        Ok((id, session))
    }

    /// La sessione valida per questo identificativo, se l'utente esiste ancora.
    pub fn get(&self, access: &Access, id: &str) -> Option<Session> {
        let mut open = self.open.lock().unwrap_or_else(|p| p.into_inner());
        let session = open.get_mut(id)?;
        let current = access.user(&session.user);
        if session.expires <= Instant::now() || current.is_none() {
            open.remove(id);
            return None;
        }
        // Il ruolo segue la configurazione: un cambio vale subito.
        session.role = current?.role;
        Some(session.clone())
    }

    pub fn logout(&self, id: &str) {
        self.open
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id);
    }
}

/// Legge il cookie di sessione dall'intestazione `Cookie`.
pub fn session_cookie(header: Option<&String>) -> Option<&str> {
    header?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value)
}

pub const COOKIE: &str = "loglem_session";

/// Chiede una password nel terminale senza mostrarla.
pub fn read_password(prompt: &str) -> Result<String> {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
    use std::io::Write;
    if let Ok(password) = std::env::var("LOGLEM_PASSWORD") {
        return Ok(password);
    }
    print!("{prompt}");
    std::io::stdout().flush()?;
    crossterm::terminal::enable_raw_mode()
        .context("Serve un terminale per inserire la password")?;
    let mut password = String::new();
    let result = loop {
        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break Ok(()),
                KeyCode::Backspace => {
                    password.pop();
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err(anyhow::anyhow!("Annullato"))
                }
                KeyCode::Char(c) => password.push(c),
                _ => {}
            },
            Ok(_) => {}
            Err(error) => break Err(error.into()),
        }
    };
    crossterm::terminal::disable_raw_mode()?;
    println!();
    result.map(|()| password)
}
