//! `loglem tail`: mostra le righe nuove appena arrivano, in locale o da un altro computer.
use crate::{collector, model::SearchRow, util};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

fn print(row: &SearchRow) {
    println!(
        "{} [{}] {}",
        row.timestamp,
        row.source,
        util::safe_text(&row.message)
    );
}

// Ctrl+C chiude subito il programma: non c'è nulla da salvare, perché la raccolta
// registra ogni blocco di righe insieme al suo punto di lettura.

/// Sullo stesso computer: legge direttamente gli archivi. Se il servizio non è attivo,
/// raccoglie anche i log, così la diretta funziona comunque.
pub fn local(dir: &Path, source: Option<&str>, text: &str, history: usize) -> Result<()> {
    for row in collector::search(dir, source, text, i64::MIN, i64::MAX, history)?
        .iter()
        .rev()
    {
        print(row);
    }
    let (_, mut after) = collector::tail(dir, source, text, None)?;
    eprintln!("— in diretta; Ctrl+C per uscire —");
    loop {
        // Fallisce in silenzio se un'altra istanza sta già raccogliendo: va bene così.
        if crate::util::Lock::acquire(&dir.join("service.lock")).is_ok() {
            let _ = collector::once(dir);
        }
        let (rows, next) = collector::tail(dir, source, text, Some(&after))?;
        rows.iter().for_each(print);
        after = next;
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Da un altro computer: entra nella pagina web del server con nome e password.
pub fn remote(server: &str, user: &str, source: Option<&str>, text: &str) -> Result<()> {
    let server = server.trim_end_matches('/');
    if !server.starts_with("http://") && !server.starts_with("https://") {
        bail!("Indica l'indirizzo completo, per esempio http://10.0.0.1:8790");
    }
    let password = crate::access::read_password(&format!("Password di {user}: "))?;
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(30))
        .build();
    let login = post(
        &agent,
        &format!("{server}/api/login"),
        &[],
        json!({"user": user, "password": password}),
    )?;
    let cookie = login
        .header("set-cookie")
        .and_then(|c| c.split(';').next())
        .context("Il server non ha aperto una sessione")?
        .to_owned();
    let token = read_json(login)?["token"]
        .as_str()
        .context("Il server non ha restituito il token di sessione")?
        .to_owned();

    let headers = [
        ("Cookie", cookie.as_str()),
        ("X-Loglem-Token", token.as_str()),
    ];
    let call = |body: Value| -> Result<Value> {
        read_json(post(&agent, &format!("{server}/api/tail"), &headers, body)?)
    };
    let mut after = call(json!({"source": source, "text": text, "after": null}))?["after"].clone();
    eprintln!("— collegato a {server}, in diretta; Ctrl+C per uscire —");
    loop {
        let reply = call(json!({"source": source, "text": text, "after": after}))?;
        let rows: Vec<SearchRow> = serde_json::from_value(reply["rows"].clone())?;
        rows.iter().for_each(print);
        after = reply["after"].clone();
    }
}

fn post(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> Result<ureq::Response> {
    let mut request = agent.post(url).set("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.set(name, value);
    }
    request.send_string(&body.to_string()).map_err(describe)
}

fn read_json(response: ureq::Response) -> Result<Value> {
    serde_json::from_reader(response.into_reader()).context("Risposta del server non valida")
}

/// Messaggio leggibile per gli errori HTTP (password sbagliata, permessi, rete).
fn describe(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(_, response) => {
            let body: Value = serde_json::from_reader(response.into_reader()).unwrap_or_default();
            anyhow::anyhow!(
                "{}",
                body["error"]
                    .as_str()
                    .unwrap_or("Il server ha rifiutato la richiesta")
            )
        }
        ureq::Error::Transport(error) => anyhow::anyhow!("Server non raggiungibile: {error}"),
    }
}
