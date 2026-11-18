//! Cosa c'è in un log: indirizzi IP, email, nomi utente, credenziali, numeri di carta,
//! accessi al server. Legge un campione delle righe archiviate e conta **quante righe**
//! contengono ogni tipo di dato. Non salva né restituisce i valori trovati.
use crate::archive::Archive;
use anyhow::Result;
use serde::Serialize;
use std::net::IpAddr;

/// Quante righe recenti guardare: abbastanza per capire il formato, poche per essere veloci.
pub const SAMPLE: usize = 5000;

/// Numero di righe del campione che contengono ogni tipo di dato.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Findings {
    pub lines_read: usize,
    pub ip: usize,
    pub email: usize,
    pub user: usize,
    pub secret: usize,
    pub card: usize,
    pub admin_access: usize,
}

impl Findings {
    /// Il log contiene dati che possono riguardare una persona.
    pub fn personal(&self) -> bool {
        self.ip + self.email + self.user + self.secret + self.card + self.admin_access > 0
    }

    pub fn add_line(&mut self, line: &str) {
        self.lines_read += 1;
        let lower = line.to_ascii_lowercase();
        self.ip += usize::from(has_ip(line));
        self.email += usize::from(has_email(line));
        self.user += usize::from(has_user(&lower));
        self.secret += usize::from(has_secret(&lower));
        self.card += usize::from(has_card(line));
        self.admin_access += usize::from(is_admin_access(&lower));
    }
}

/// Le righe più recenti dell'archivio.
pub fn scan(store: &Archive) -> Result<Findings> {
    let mut stmt = store
        .db
        .prepare("SELECT message FROM log_rows ORDER BY id DESC LIMIT ?1")?;
    let mut rows = stmt.query([SAMPLE as i64])?;
    let mut found = Findings::default();
    while let Some(row) = rows.next()? {
        let message: String = row.get(0)?;
        found.add_line(&message);
    }
    Ok(found)
}

pub fn scan_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Findings {
    let mut found = Findings::default();
    for line in lines {
        found.add_line(line);
    }
    found
}

/// Un indirizzo IP che può indicare una persona: esclusi `127.0.0.1`, `::1` e `0.0.0.0`.
/// Le stesse regole del mascheramento: pezzi fatti di cifre, punti e due punti.
fn has_ip(line: &str) -> bool {
    line.split(|c: char| !(c.is_ascii_hexdigit() || c == '.' || c == ':'))
        .any(|token| {
            let token = token.trim_end_matches(['.', ':']);
            let ip = token.parse::<IpAddr>().ok().or_else(|| {
                let (host, port) = token.rsplit_once(':')?;
                port.bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| host.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4))?
            });
            ip.is_some_and(|ip| !ip.is_loopback() && !ip.is_unspecified())
        })
}

fn has_email(line: &str) -> bool {
    let bytes = line.as_bytes();
    let local = |b: u8| b.is_ascii_alphanumeric() || b"._%+-".contains(&b);
    let domain = |b: u8| b.is_ascii_alphanumeric() || b == b'.' || b == b'-';
    bytes.iter().enumerate().any(|(at, &b)| {
        if b != b'@' || at == 0 || !local(bytes[at - 1]) {
            return false;
        }
        let rest = &line[at + 1..];
        let host: &str = &rest[..rest.bytes().position(|b| !domain(b)).unwrap_or(rest.len())];
        let host = host.trim_end_matches('.');
        host.rsplit_once('.').is_some_and(|(name, tld)| {
            !name.is_empty() && tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic())
        })
    })
}

/// Dopo `chiave` c'è un valore vero (non vuoto, non già oscurato)?
/// `lower` è la riga in minuscolo; la chiave deve iniziare dopo un carattere che non
/// faccia parte di una parola (`sid=` non deve trovare `pid=` o `said=`).
fn has_value_after(lower: &str, key: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = lower[from..].find(key) {
        let start = from + pos;
        from = start + key.len();
        let before = lower[..start].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let value = lower[from..].trim_start_matches(['"', ':', '=', '[', ' ']);
        let empty = value.is_empty()
            || value.starts_with(['"', '&', ',', ']', '}', ' '])
            || value.starts_with("null");
        let hidden = ["redacted", "***", "[filtered]", "xxx", "<hidden>", "-"]
            .iter()
            .any(|h| value.starts_with(h));
        if !empty && !hidden {
            return true;
        }
    }
    false
}

fn has_user(lower: &str) -> bool {
    [
        "user=",
        "username=",
        "userid=",
        "login=",
        "\"user\":",
        "\"username\":",
        "\"user_id\":",
        "\"email\":",
        "for user ",
        "for invalid user ",
        "user ",
    ]
    .iter()
    .any(|key| {
        if *key == "user " {
            // Solo nelle forme dei log di sistema: «session opened for user …», «user admin».
            lower.contains("sudo:") && has_value_after(lower, key)
        } else {
            has_value_after(lower, key)
        }
    })
}

fn has_secret(lower: &str) -> bool {
    [
        "password=",
        "passwd=",
        "pwd=",
        "\"password\":",
        "\"passwd\":",
        "token=",
        "\"token\":",
        "access_token=",
        "\"access_token\":",
        "refresh_token=",
        "\"refresh_token\":",
        "api_key=",
        "\"api_key\":",
        "apikey=",
        "secret=",
        "\"secret\":",
        "authorization: bearer ",
        "\"authorization\":",
        "phpsessid=",
        "jsessionid=",
        "sessionid=",
        "sid=",
        "\"cookie\":",
        "set-cookie:",
    ]
    .iter()
    .any(|key| has_value_after(lower, key))
}

/// Numero di carta scritto a gruppi di quattro cifre (`4111 1111 1111 1111` o con trattini)
/// che passa il controllo di Luhn. Le sequenze di cifre senza separatori non contano: date e
/// identificativi lunghi darebbero troppi falsi allarmi.
fn has_card(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() || (i > 0 && bytes[i - 1].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let mut digits = Vec::new();
        let mut j = i;
        let mut separator = None;
        loop {
            let group_start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits.push(bytes[j] - b'0');
                j += 1;
            }
            let group = j - group_start;
            let next = bytes.get(j).copied();
            let continues = group == 4
                && matches!(next, Some(b' ' | b'-'))
                && separator.is_none_or(|s| Some(s) == next)
                && bytes.get(j + 1).is_some_and(u8::is_ascii_digit);
            if continues {
                separator = next;
                j += 1;
            } else {
                break;
            }
        }
        if separator.is_some() && (13..=19).contains(&digits.len()) && luhn(&digits) {
            return true;
        }
        i = j.max(i + 1);
    }
    false
}

fn luhn(digits: &[u8]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(n, &d)| {
            let d = u32::from(d);
            if n % 2 == 1 {
                if d * 2 > 9 {
                    d * 2 - 9
                } else {
                    d * 2
                }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Righe degli accessi al server: SSH, sudo, login di sistema (testo o journal in JSON).
fn is_admin_access(lower: &str) -> bool {
    [
        "sshd[",
        "sshd:",
        "\"sshd\"",
        "accepted publickey",
        "accepted password",
        "failed password",
        "sudo:",
        "\"sudo\"",
        "pam_unix(",
        "session opened for user",
    ]
    .iter()
    .any(|mark| lower.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Findings {
        scan_lines([line])
    }

    #[test]
    fn ip_addresses() {
        assert_eq!(
            one("203.0.113.7 - - [17/Nov/2026] \"GET / HTTP/1.1\" 200").ip,
            1
        );
        assert_eq!(one("{\"client_ip\":\"2001:db8::1\"}").ip, 1);
        assert_eq!(one("connect from 198.51.100.4:51820").ip, 1);
        assert_eq!(one("listening on 127.0.0.1:8790 and ::1").ip, 0);
        assert_eq!(
            one("2026-11-17T15:31:14.049Z version 1.2.3 took 15:31").ip,
            0
        );
    }

    #[test]
    fn emails() {
        assert_eq!(one("reset sent to anna.rossi@example.com").email, 1);
        assert_eq!(one("ssh admin@server1 failed").email, 0);
        assert_eq!(one("price 10@ shop").email, 0);
    }

    #[test]
    fn users_need_a_value() {
        assert_eq!(one("login ok user=demo ip=203.0.113.4").user, 1);
        assert_eq!(one("{\"user_id\":\"\",\"status\":200}").user, 0);
        assert_eq!(one("{\"username\":\"anna\"}").user, 1);
        assert_eq!(
            one("Failed password for invalid user admin from 203.0.113.9").user,
            1
        );
    }

    #[test]
    fn secrets_but_not_redacted_ones() {
        assert_eq!(one("GET /reset?token=abc123 HTTP/1.1").secret, 1);
        assert_eq!(one("{\"password\":\"hunter2\"}").secret, 1);
        assert_eq!(one("Authorization: Bearer eyJhbGciOi").secret, 1);
        assert_eq!(
            one("{\"Cookie\":[\"REDACTED\"],\"Authorization\":[\"REDACTED\"]}").secret,
            0
        );
        assert_eq!(one("worker pid=4242 said=hello").secret, 0);
        assert_eq!(one("csrf token missing").secret, 0);
    }

    #[test]
    fn cards_need_groups_and_luhn() {
        assert_eq!(one("paid with 4111 1111 1111 1111").card, 1);
        assert_eq!(one("paid with 4111-1111-1111-1111").card, 1);
        assert_eq!(one("paid with 4111 1111 1111 1112").card, 0);
        assert_eq!(one("ts 1794929474062054600 id 4111111111111111").card, 0);
    }

    #[test]
    fn admin_access() {
        assert_eq!(
            one("Nov 17 sshd[812]: Accepted publickey for admin from 203.0.113.2").admin_access,
            1
        );
        assert_eq!(
            one("{\"SYSLOG_IDENTIFIER\":\"sudo\",\"MESSAGE\":\"admin : COMMAND=/bin/ls\"}")
                .admin_access,
            1
        );
        assert_eq!(one("GET /login 200").admin_access, 0);
    }

    #[test]
    fn nothing_personal() {
        let found = scan_lines([
            "2026-11-17 INFO cache warmed in 12 ms",
            "ERROR disk full on /var",
        ]);
        assert_eq!(found.lines_read, 2);
        assert!(!found.personal());
    }
}
