//! Suggerimenti per trattare i log nel modo giusto: Loglem guarda cosa c'è nel log e nella
//! configurazione, propone la scelta prudente con il riferimento di legge e lascia decidere.
//!
//! Le regole (quando compare un suggerimento, quando è già fatto, quale azione propone) stanno
//! qui; i testi stanno in `i18n/advice.<lingua>.json`, con la data di revisione e l'indicazione
//! se i riferimenti sono stati verificati da un esperto.
use crate::{
    archive::Archive,
    config,
    findings::{self, Findings},
    model::{Retention, RetentionUnit, Source},
    util,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub const LANGUAGES: &[(&str, &str)] = &[
    ("it", include_str!("i18n/advice.it.json")),
    ("en", include_str!("i18n/advice.en.json")),
];

/// Per ora la pagina è in italiano; la scelta della lingua arriverà con le traduzioni.
pub const DEFAULT_LANGUAGE: &str = "it";

/// Un «rimandato» torna dopo una settimana.
const LATER_DAYS: i64 = 7;

/// La durata proposta per i log con dati personali e il minimo per gli accessi al server.
const SIX_MONTHS: Retention = Retention {
    amount: 6,
    unit: RetentionUnit::Months,
};

/// Oltre questa durata (in giorni, circa) un log con dati personali riceve un suggerimento.
const LONG_DAYS: u32 = 365;

/// La scelta dell'utente su un suggerimento, salvata nella sorgente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// `applied`, `done`, `declined` o `later`.
    pub choice: String,
    pub at: String,
    pub by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    MaskIps,
    SixMonths,
    OpenPrivacy,
    None,
}

impl Action {
    fn code(self) -> Option<&'static str> {
        match self {
            Action::MaskIps => Some("mask_ips"),
            Action::SixMonths => Some("retention"),
            Action::OpenPrivacy => Some("open_privacy"),
            Action::None => None,
        }
    }
}

/// Un suggerimento che vale per questa sorgente, prima di aggiungere i testi.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    id: &'static str,
    important: bool,
    /// Righe del campione che lo fanno scattare (0 se dipende solo dalla configurazione).
    lines: usize,
    /// Già soddisfatto dalla configurazione attuale.
    satisfied: bool,
    action: Action,
}

fn days(period: Retention) -> u32 {
    match period.unit {
        RetentionUnit::Days => period.amount,
        RetentionUnit::Months => period.amount.saturating_mul(30),
        RetentionUnit::Years => period.amount.saturating_mul(365),
    }
}

/// Quali suggerimenti valgono, dati cosa c'è nel log e come è configurata la sorgente.
fn rules(source: &Source, found: &Findings) -> Vec<Rule> {
    let period = days(source.period());
    let rule = |id, important, lines, satisfied, action| Rule {
        id,
        important,
        lines,
        satisfied,
        action,
    };
    let mut out = Vec::new();
    if found.secret > 0 {
        out.push(rule(
            "secret",
            true,
            found.secret,
            false,
            Action::OpenPrivacy,
        ));
    }
    if found.card > 0 {
        out.push(rule("card", true, found.card, false, Action::OpenPrivacy));
    }
    if found.admin_access > 0 {
        let enough = period >= days(SIX_MONTHS);
        out.push(rule(
            "admin_access",
            false,
            found.admin_access,
            enough,
            Action::SixMonths,
        ));
    }
    if found.ip > 0 {
        let masked = source.privacy.mask_ips_after_days.is_some();
        out.push(rule("ip", false, found.ip, masked, Action::MaskIps));
    }
    // Per gli accessi al server una durata lunga è richiesta, non un problema.
    if found.personal() && found.admin_access == 0 && period > LONG_DAYS {
        out.push(rule("retention_long", false, 0, false, Action::SixMonths));
    }
    if found.email + found.user > 0 {
        out.push(rule(
            "user",
            false,
            found.email.max(found.user),
            false,
            Action::OpenPrivacy,
        ));
    }
    if found.personal() {
        out.push(rule("notice", false, 0, false, Action::None));
    }
    out
}

fn catalog(language: &str) -> Value {
    let text = LANGUAGES
        .iter()
        .find(|(code, _)| *code == language)
        .or_else(|| LANGUAGES.iter().find(|(code, _)| *code == DEFAULT_LANGUAGE))
        .map(|(_, text)| *text)
        .unwrap_or_default();
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn fill(text: &str, values: &[(&str, String)]) -> String {
    values.iter().fold(text.to_owned(), |text, (key, value)| {
        text.replace(&format!("{{{key}}}"), value)
    })
}

fn state_of(rule: &Rule, choice: Option<&Choice>, now_ns: i64) -> &'static str {
    if rule.satisfied {
        return "done";
    }
    match choice.map(|c| c.choice.as_str()) {
        Some("done" | "applied") => "done",
        Some("declined") => "declined",
        Some("later") => {
            let at = choice
                .and_then(|c| crate::timestamps::parse_date(&c.at, false).ok())
                .unwrap_or(i64::MIN);
            if now_ns.saturating_sub(at) < LATER_DAYS * 86_400 * 1_000_000_000 {
                "later"
            } else {
                "open"
            }
        }
        _ => "open",
    }
}

/// Cosa c'è nel log di una sorgente (campione delle righe recenti). `None` se non ha ancora
/// righe archiviate.
pub fn findings_of(dir: &Path, source: &Source) -> Result<Option<Findings>> {
    let path = source.archive_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    let _access = util::DataAccess::read(dir)?;
    let found = findings::scan(&Archive::open(&path)?)?;
    Ok((found.lines_read > 0).then_some(found))
}

/// Suggerimenti di una sorgente, con i testi nella lingua chiesta.
pub fn report(dir: &Path, name: &str, language: &str) -> Result<Value> {
    let source = config::source(dir, name)?;
    let found = findings_of(dir, &source)?;
    let texts = catalog(language);
    let now = crate::timestamps::now_ns();
    let period = source.period();
    let items: Vec<Value> = found
        .as_ref()
        .map(|found| rules(&source, found))
        .unwrap_or_default()
        .into_iter()
        .map(|rule| {
            let t = &texts["items"][rule.id];
            let count = if rule.lines == 1 { "one" } else { "many" };
            let lines = texts["lines"][count]
                .as_str()
                .unwrap_or("{n}")
                .replace("{n}", &rule.lines.to_string());
            let values = [
                ("lines", lines),
                (
                    "sample",
                    found.as_ref().map_or(0, |f| f.lines_read).to_string(),
                ),
                ("period", period_text(period, language)),
                ("proposal", period_text(SIX_MONTHS, language)),
            ];
            let field = |key: &str| fill(t[key].as_str().unwrap_or(""), &values);
            let refs: Vec<Value> = t["refs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    let law = &texts["laws"][r["law"].as_str().unwrap_or("")];
                    json!({"text": r["text"], "law": law["name"], "url": law["url"]})
                })
                .collect();
            let choice = source.advice.get(rule.id);
            json!({
                "id": rule.id,
                "important": rule.important,
                "state": state_of(&rule, choice, now),
                "choice": choice,
                "title": field("title"),
                "why": field("why"),
                "proposal": field("proposal"),
                "verify": field("verify"),
                "done_text": field("done"),
                "refs": refs,
                "action": rule.action.code(),
                "action_label": field("action"),
            })
        })
        .collect();
    Ok(json!({
        "source": source.id,
        "findings": found,
        "items": items,
        "disclaimer": texts["disclaimer"],
        "unverified": (!texts["verified"].as_bool().unwrap_or(false)).then(|| texts["unverified"].clone()),
        "revised": texts["revised"],
    }))
}

fn period_text(period: Retention, language: &str) -> String {
    if language == "it" {
        return period.to_string();
    }
    let unit = match (period.unit, period.amount == 1) {
        (RetentionUnit::Days, true) => "day",
        (RetentionUnit::Days, false) => "days",
        (RetentionUnit::Months, true) => "month",
        (RetentionUnit::Months, false) => "months",
        (RetentionUnit::Years, true) => "year",
        (RetentionUnit::Years, false) => "years",
    };
    format!("{} {unit}", period.amount)
}

fn record(dir: &Path, name: &str, id: &str, choice: &str, who: &str) -> Result<()> {
    let at = crate::timestamps::format_ns(crate::timestamps::now_ns());
    config::edit(dir, |config| {
        let source = config
            .sources
            .iter_mut()
            .find(|s| s.id == name)
            .context("Source not found")?;
        source.advice.insert(
            id.to_owned(),
            Choice {
                choice: choice.to_owned(),
                at,
                by: who.to_owned(),
            },
        );
        Ok(())
    })?;
    crate::audit::record(
        dir,
        who,
        "suggerimento",
        &format!("sorgente: {name}; {id}: {choice}"),
    );
    Ok(())
}

/// Registra «fatto», «non mi riguarda» o «ricordamelo più tardi»; `reopen` toglie la scelta.
pub fn choose(dir: &Path, name: &str, id: &str, choice: &str, who: &str) -> Result<()> {
    known(id)?;
    match choice {
        "done" | "declined" | "later" => record(dir, name, id, choice, who),
        "reopen" => {
            config::edit(dir, |config| {
                let source = config
                    .sources
                    .iter_mut()
                    .find(|s| s.id == name)
                    .context("Source not found")?;
                source.advice.remove(id);
                Ok(())
            })?;
            crate::audit::record(
                dir,
                who,
                "suggerimento",
                &format!("sorgente: {name}; {id}: riaperto"),
            );
            Ok(())
        }
        _ => bail!("Scelta non valida"),
    }
}

/// Applica la proposta di un suggerimento. Il server decide cosa fare a partire dall'`id`:
/// la pagina non può chiedere altro.
pub fn apply(dir: &Path, name: &str, id: &str, who: &str) -> Result<()> {
    let source = config::source(dir, name)?;
    let found = findings_of(dir, &source)?.unwrap_or_default();
    let rule = rules(&source, &found)
        .into_iter()
        .find(|r| r.id == id)
        .context("Questo suggerimento non vale più per la sorgente")?;
    match rule.action {
        Action::MaskIps => config::edit(dir, |config| {
            let source = config
                .sources
                .iter_mut()
                .find(|s| s.id == name)
                .context("Source not found")?;
            source.privacy.mask_ips_after_days = Some(0);
            source.privacy.validate(source)
        })?,
        Action::SixMonths => {
            crate::management::apply(
                dir,
                crate::management::Change {
                    name: name.to_owned(),
                    retention: Some(SIX_MONTHS),
                    ..Default::default()
                },
            )?;
        }
        Action::OpenPrivacy | Action::None => {
            bail!("Questo suggerimento non ha un'azione da applicare")
        }
    }
    record(dir, name, id, "applied", who)
}

fn known(id: &str) -> Result<()> {
    let texts = catalog(DEFAULT_LANGUAGE);
    if texts["items"].get(id).is_none() {
        bail!("Suggerimento sconosciuto");
    }
    Ok(())
}

/// Testo per il terminale.
pub fn text(dir: &Path, name: &str, language: &str) -> Result<String> {
    let report = report(dir, name, language)?;
    let mut out = Vec::new();
    match report["findings"].as_object() {
        None => out.push("Ancora nessuna riga letta: riprova dopo la prima raccolta.".to_owned()),
        Some(_) if report["items"].as_array().is_none_or(Vec::is_empty) => {
            out.push("Nelle righe recenti non ho trovato dati personali.".to_owned())
        }
        Some(_) => {}
    }
    for item in report["items"].as_array().into_iter().flatten() {
        let state = match item["state"].as_str() {
            Some("done") => "[fatto] ",
            Some("declined") => "[non riguarda] ",
            Some("later") => "[rimandato] ",
            _ if item["important"].as_bool() == Some(true) => "[IMPORTANTE] ",
            _ => "",
        };
        out.push(format!("\n{state}{}", item["title"].as_str().unwrap_or("")));
        for key in ["why", "proposal", "verify"] {
            out.push(format!("  {}", item[key].as_str().unwrap_or("")));
        }
        for r in item["refs"].as_array().into_iter().flatten() {
            out.push(format!(
                "  → {} {}: {}",
                r["law"].as_str().unwrap_or(""),
                r["text"].as_str().unwrap_or(""),
                r["url"].as_str().unwrap_or("")
            ));
        }
    }
    out.push(String::new());
    out.push(report["disclaimer"].as_str().unwrap_or("").to_owned());
    if let Some(note) = report["unverified"].as_str() {
        out.push(note.to_owned());
    }
    Ok(out.join("\n"))
}

/// Tutte le lingue hanno gli stessi suggerimenti, gli stessi campi e gli stessi segnaposto.
/// Usato da `loglem self-check`.
pub fn check_catalogs() -> Result<()> {
    let reference = catalog(DEFAULT_LANGUAGE);
    let ids: BTreeMap<_, _> = reference["items"]
        .as_object()
        .context("Catalogo dei suggerimenti senza voci")?
        .iter()
        .collect();
    for rule_id in [
        "ip",
        "retention_long",
        "user",
        "secret",
        "card",
        "admin_access",
        "notice",
    ] {
        if !ids.contains_key(&rule_id.to_owned()) {
            bail!("Manca il testo del suggerimento {rule_id}");
        }
    }
    for (language, text) in LANGUAGES {
        let other: Value = serde_json::from_str(text)
            .with_context(|| format!("advice.{language}.json non è JSON valido"))?;
        for (id, item) in &ids {
            for field in ["title", "why", "proposal", "verify", "action", "done"] {
                let a = item[field].as_str().unwrap_or("");
                let b = other["items"][id.as_str()][field]
                    .as_str()
                    .with_context(|| format!("advice.{language}.json: manca {id}.{field}"))?;
                let marks = |s: &str| {
                    let mut m: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}').map(|(k, _)| k.to_owned()))
                        .collect();
                    m.sort();
                    m
                };
                if marks(a) != marks(b) {
                    bail!("advice.{language}.json: segnaposto diversi in {id}.{field}");
                }
            }
            for count in ["one", "many"] {
                if !other["lines"][count]
                    .as_str()
                    .is_some_and(|s| s.contains("{n}"))
                {
                    bail!("advice.{language}.json: manca lines.{count}");
                }
            }
            for r in other["items"][id.as_str()]["refs"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let law = r["law"].as_str().unwrap_or("");
                if other["laws"][law]["url"].as_str().is_none() {
                    bail!("advice.{language}.json: legge sconosciuta {law} in {id}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Input;

    fn source(days: u32) -> Source {
        Source {
            id: "web".into(),
            retention_days: days,
            enabled: true,
            input: Input::Folder {
                path: "/tmp".into(),
            },
            ..Source::default()
        }
    }

    fn ids(rules: &[Rule]) -> Vec<&'static str> {
        rules.iter().map(|r| r.id).collect()
    }

    #[test]
    fn catalogs_are_complete() {
        check_catalogs().unwrap();
    }

    #[test]
    fn nothing_personal_no_advice() {
        let found = findings::scan_lines(["INFO cache warmed", "ERROR disk full"]);
        assert!(rules(&source(28), &found).is_empty());
    }

    #[test]
    fn access_log_with_ips() {
        let found = findings::scan_lines(["203.0.113.7 - - \"GET / HTTP/1.1\" 200"]);
        let r = rules(&source(28), &found);
        assert_eq!(ids(&r), ["ip", "notice"]);
        assert!(!r[0].satisfied);
        let mut masked = source(28);
        masked.privacy.mask_ips_after_days = Some(0);
        assert!(rules(&masked, &found)[0].satisfied);
        // Due anni: compare anche la durata.
        assert_eq!(
            ids(&rules(&source(730), &found)),
            ["ip", "retention_long", "notice"]
        );
    }

    #[test]
    fn credentials_come_first_and_are_important() {
        let found = findings::scan_lines(["GET /reset?token=abc user=anna 203.0.113.9"]);
        let r = rules(&source(28), &found);
        assert_eq!(ids(&r), ["secret", "ip", "user", "notice"]);
        assert!(r[0].important);
    }

    #[test]
    fn server_logins_want_six_months() {
        let found =
            findings::scan_lines(["sshd[1]: Accepted publickey for deploy from 203.0.113.2"]);
        let short = rules(&source(28), &found);
        assert_eq!(short[0].id, "admin_access");
        assert!(!short[0].satisfied);
        let mut long = source(28);
        long.retention = Some(SIX_MONTHS);
        assert!(rules(&long, &found)[0].satisfied);
        // Una durata lunga qui è voluta: niente «retention_long».
        assert!(!ids(&rules(&source(730), &found)).contains(&"retention_long"));
    }

    #[test]
    fn later_comes_back_after_a_week() {
        let rule = Rule {
            id: "ip",
            important: false,
            lines: 1,
            satisfied: false,
            action: Action::MaskIps,
        };
        let now = crate::timestamps::now_ns();
        let choice = |days_ago: i64| Choice {
            choice: "later".into(),
            at: crate::timestamps::format_ns(now - days_ago * 86_400 * 1_000_000_000),
            by: "anna".into(),
        };
        assert_eq!(state_of(&rule, Some(&choice(1)), now), "later");
        assert_eq!(state_of(&rule, Some(&choice(8)), now), "open");
        assert_eq!(state_of(&rule, None, now), "open");
    }
}
