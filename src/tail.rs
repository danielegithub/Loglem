//! `loglem tail`: mostra le righe nuove appena arrivano, in locale o da un altro computer.
use crate::{collector, model::SearchRow, query::Query, util};
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
pub fn local(dir: &Path, source: Option<&str>, query: &Query, history: usize) -> Result<()> {
    for row in collector::search(dir, source, query, i64::MIN, i64::MAX, history)?
        .iter()
        .rev()
    {
        print(row);
    }
    let (_, mut after) = collector::tail(dir, source, query, None)?;
    eprintln!("— in diretta; Ctrl+C per uscire —");
    loop {
        // Fallisce in silenzio se un'altra istanza sta già raccogliendo: va bene così.
        if crate::util::Lock::acquire(&dir.join("service.lock")).is_ok() {
            let _ = collector::once(dir);
        }
        let (rows, next) = collector::tail(dir, source, query, Some(&after))?;
        rows.iter().for_each(print);
        after = next;
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Da un altro computer: entra nella pagina web del server con nome e password.
pub fn remote(server: &str, user: &str, source: Option<&str>, query: &Query) -> Result<()> {
    let server = server.trim_end_matches('/');
    if !server.starts_with("http://") && !server.starts_with("https://") {
        bail!("Indica l'indirizzo completo, per esempio http://10.0.0.1:8790");
    }
    let password = crate::access::read_password(&format!("Password di {user}: "))?;
    let mut reply = Vec::new();
    let cookie = crate::http::transfer(
        &format!("{server}/api/login"),
        &[("Content-Type", "application/json")],
        Some(&serde_json::to_vec(
            &json!({"user":user,"password":password}),
        )?),
        65536,
        &mut reply,
    )?
    .context("Il server non ha aperto una sessione")?;
    let login: Value = serde_json::from_slice(&reply)?;
    let token = login["token"]
        .as_str()
        .context("Il server non ha restituito il token di sessione")?
        .to_owned();

    let headers = [
        ("Content-Type", "application/json"),
        ("Cookie", cookie.as_str()),
        ("X-Loglem-Token", token.as_str()),
    ];
    let call = |body: Value| -> Result<Value> {
        let mut reply = Vec::new();
        crate::http::transfer(
            &format!("{server}/api/tail"),
            &headers,
            Some(&serde_json::to_vec(&body)?),
            64 * 1024 * 1024,
            &mut reply,
        )?;
        Ok(serde_json::from_slice(&reply)?)
    };
    // Stessi filtri della ricerca, più la sorgente e il punto di ripartenza.
    let request = |after: &Value| -> Result<Value> {
        let mut body = serde_json::to_value(query)?;
        body["source"] = json!(source);
        body["after"] = after.clone();
        call(body)
    };
    let mut after = request(&Value::Null)?["after"].clone();
    eprintln!("— collegato a {server}, in diretta; Ctrl+C per uscire —");
    loop {
        let reply = request(&after)?;
        let rows: Vec<SearchRow> = serde_json::from_value(reply["rows"].clone())?;
        rows.iter().for_each(print);
        after = reply["after"].clone();
    }
}
