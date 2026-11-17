//! Controlli sui dati personali: mascheramento degli IP e campi JSON da tenere.
//!
//! Aiutano a rispettare le regole sulla conservazione, ma da soli non rendono conforme
//! un'installazione: decidere cosa conservare e perché resta compito di chi la gestisce.
use crate::{archive::Archive, config, model::Source};
use anyhow::{bail, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::net::IpAddr;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Privacy {
    /// Maschera gli indirizzi IP delle righe più vecchie di così (0 = appena lette).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_ips_after_days: Option<u32>,
    /// Nelle righe JSON tiene solo questi campi (es. `request.uri`, `status`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep_fields: Vec<String>,
}

impl Privacy {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self, source: &Source) -> Result<()> {
        // Il controllo prima di cancellare un originale usa i byte letti dal file, non il testo
        // salvato: mascherare o ridurre i campi non lo disturba.
        if source.proof.is_some() && self.mask_ips_after_days.is_some_and(|d| d > 0) {
            // Mascherare dopo il sigillo cambierebbe righe già sigillate.
            bail!(
                "{}: in modalità prova gli IP si possono mascherare solo subito \
                 (--mask-ips-after-days 0), prima del sigillo",
                source.id
            );
        }
        Ok(())
    }
}

/// Sostituisce gli indirizzi IP: IPv4 senza l'ultimo numero, IPv6 solo i primi 48 bit.
pub fn mask_ips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        out.push_str(&mask_token(token));
        token.clear();
    };
    for c in text.chars() {
        if c.is_ascii_hexdigit() || c == '.' || c == ':' {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Un pezzo di testo fatto di cifre, punti e due punti: se è un IP lo maschera.
/// Toglie prima un eventuale `:porta` finale o punto di fine frase.
fn mask_token(token: &str) -> String {
    let trimmed = token.trim_end_matches(['.', ':']);
    let tail = &token[trimmed.len()..];
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return format!("{}{tail}", mask_ip(ip));
    }
    if let Some((host, port)) = trimmed.rsplit_once(':') {
        if let (Ok(IpAddr::V4(ip)), true) = (
            host.parse::<IpAddr>(),
            port.bytes().all(|b| b.is_ascii_digit()),
        ) {
            return format!("{}:{port}{tail}", mask_ip(IpAddr::V4(ip)));
        }
    }
    token.to_owned()
}

fn mask_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            format!("{a}.{b}.{c}.0")
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}::", s[0], s[1], s[2])
        }
    }
}

/// Tiene solo i campi indicati (anche annidati: `request.uri`) di una riga JSON.
pub fn keep_json_fields(text: &str, fields: &[String]) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let mut kept = Map::new();
    for field in fields {
        let path: Vec<&str> = field.split('.').collect();
        if let Some(found) = path.iter().try_fold(&value, |v, key| v.get(key)) {
            insert_path(&mut kept, &path, found.clone());
        }
    }
    serde_json::to_string(&Value::Object(kept)).ok()
}

fn insert_path(map: &mut Map<String, Value>, path: &[&str], value: Value) {
    match path {
        [last] => {
            map.insert((*last).to_owned(), value);
        }
        [first, rest @ ..] => {
            let child = map
                .entry((*first).to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(child) = child {
                insert_path(child, rest, value);
            }
        }
        [] => {}
    }
}

const DONE_UNTIL: &str = "privacy_until";
const LAST_RUN: &str = "privacy_run";

/// Applica le regole alle righe diventate abbastanza vecchie dall'ultimo giro, e a quelle
/// arrivate nel frattempo con date vecchie (per esempio storico compresso importato dopo).
pub fn apply(store: &Archive, privacy: &Privacy, now: i64) -> Result<usize> {
    let Some(days) = privacy
        .mask_ips_after_days
        .or((!privacy.keep_fields.is_empty()).then_some(0))
    else {
        return Ok(0);
    };
    let until = now - i64::from(days) * 86_400 * 1_000_000_000;
    let policy = serde_json::to_string(privacy)?;
    let changed_policy = store.get_state("privacy_policy")?.as_deref() != Some(&policy);
    let done_until: i64 = store
        .get_state(DONE_UNTIL)?
        .and_then(|s| s.parse().ok())
        .unwrap_or(i64::MIN);
    let last_run: i64 = store
        .get_state(LAST_RUN)?
        .and_then(|s| s.parse().ok())
        .unwrap_or(i64::MIN);
    let done_until = if changed_policy { i64::MIN } else { done_until };
    let mut changed = 0;
    let mut cursor = 0i64;
    loop {
        // Ordine stabile per id: una nuova classe può richiedere il trattamento di tutto lo storico.
        let tx = store.db.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "SELECT id,message FROM log_rows WHERE id>?1 AND event_ns<=?2
            AND (event_ns>?3 OR collected_ns>=?4) ORDER BY id LIMIT 128",
        )?;
        let mut rows = stmt.query(params![cursor, until, done_until, last_run])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            cursor = row.get(0)?;
            let message: String = row.get(1)?;
            let mut text = message.clone();
            if !privacy.keep_fields.is_empty() {
                text = keep_json_fields(&text, &privacy.keep_fields).unwrap_or(text);
            }
            if privacy.mask_ips_after_days.is_some() {
                text = mask_ips(&text);
            }
            if text != message {
                tx.execute(
                    "UPDATE entries SET message=?1 WHERE id=?2",
                    params![text, cursor],
                )?;
                changed += 1;
            }
            count += 1;
        }
        drop(rows);
        drop(stmt);
        tx.commit()?;
        if count < 128 {
            break;
        }
    }
    store.set_state(DONE_UNTIL, &until.max(done_until).to_string())?;
    store.set_state(LAST_RUN, &now.to_string())?;
    store.set_state("privacy_policy", &policy)?;
    Ok(changed)
}

/// Rapporto per il registro delle conservazioni: cosa si tiene, per quanto, con quali tutele.
pub fn report(dir: &std::path::Path) -> Result<String> {
    let config = config::load(dir)?;
    let statuses = crate::collector::statuses(dir)?;
    let mut lines = vec![
        "Sorgente | Tipo di log | Conservazione | IP mascherati | Campi JSON | Righe | Dalla data"
            .to_owned(),
    ];
    for source in &config.sources {
        let status = statuses.iter().find(|s| s.source == source.id);
        let kind = match source.log_kind {
            crate::model::LogKind::Access => "accessi (contiene IP)",
            crate::model::LogKind::Error => "errori",
            crate::model::LogKind::Generic => "generico (può contenere dati personali)",
        };
        let mask = match source.privacy.mask_ips_after_days {
            Some(0) => "subito".to_owned(),
            Some(days) => format!("dopo {days} giorni"),
            None => "no".to_owned(),
        };
        let fields = if source.privacy.keep_fields.is_empty() {
            "tutti".to_owned()
        } else {
            source.privacy.keep_fields.join(", ")
        };
        lines.push(format!(
            "{} | {kind} | {} | {mask} | {fields} | {} | {}",
            source.id,
            source.period(),
            status.map(|s| s.entries).unwrap_or(0),
            status
                .and_then(|s| s.earliest.clone())
                .unwrap_or_else(|| "—".into()),
        ));
    }
    lines.push(String::new());
    lines.push(
        "Nota: questi controlli aiutano la conformità, ma da soli non rendono conforme \
         un'installazione. Le copie di backup conservano anche le righe scadute."
            .into(),
    );
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_ipv4_ipv6_and_ports() {
        assert_eq!(mask_ips("from 203.0.113.7 ok"), "from 203.0.113.0 ok");
        assert_eq!(
            mask_ips("peer 198.51.100.4:51820."),
            "peer 198.51.100.0:51820."
        );
        assert_eq!(
            mask_ips("{\"ip\":\"2001:db8:85a3::8a2e:370:7334\"}"),
            "{\"ip\":\"2001:db8:85a3::\"}"
        );
    }

    #[test]
    fn leaves_other_text_alone() {
        let line = "2026-11-17T15:31:14.049Z version 1.2.3 took 15:31 id cafe";
        assert_eq!(mask_ips(line), line);
    }

    #[test]
    fn keeps_only_chosen_json_fields() {
        let line =
            r#"{"status":404,"request":{"uri":"/x","client_ip":"203.0.113.7"},"user_agent":"x"}"#;
        let kept = keep_json_fields(line, &["status".into(), "request.uri".into()]).unwrap();
        assert_eq!(kept, r#"{"request":{"uri":"/x"},"status":404}"#);
        assert_eq!(keep_json_fields("not json", &["status".into()]), None);
    }
}
