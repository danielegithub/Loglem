//! Bounded HTTP transfers. Authentication data travels through private files and stdin.
use anyhow::{ensure, Context, Result};
use std::{
    io::{Read, Seek, Write},
    process::{Command, Stdio},
};

/// Il server ha risposto, ma con un errore: il codice serve a spiegare cosa non va.
#[derive(Debug)]
pub(crate) struct HttpError(pub u16);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Richiesta HTTP rifiutata ({})", self.0)
    }
}

impl std::error::Error for HttpError {}

pub(crate) fn transfer(
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    limit: u64,
    output: &mut dyn Write,
) -> Result<Option<String>> {
    let get = body.is_none();
    let protocol = if get || url.starts_with("https://") {
        "=https"
    } else {
        "=http,https"
    };
    let mut command = Command::new("curl");
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--globoff",
        "--proto",
        protocol,
        "--proto-redir",
        "=https",
        "--max-time",
        if get { "120" } else { "30" },
    ]);
    if get {
        command.args(["--location", "--max-redirs", "5"]);
    }
    let mut request_headers = tempfile::NamedTempFile::new()?;
    for (name, value) in headers {
        ensure!(!value.contains(['\r', '\n']), "Invalid HTTP header");
        writeln!(request_headers, "{name}: {value}")?;
    }
    let response_headers = tempfile::NamedTempFile::new()?;
    command
        .arg("--header")
        .arg(format!("@{}", request_headers.path().display()))
        .arg("--dump-header")
        .arg(response_headers.path());
    if let Some(bytes) = body {
        let mut input = tempfile::tempfile()?;
        input.write_all(bytes)?;
        input.rewind()?;
        command.args(["--data-binary", "@-"]).stdin(input);
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command
        .args(["--url", url])
        .stdout(Stdio::piped())
        .spawn()
        .context("Impossibile avviare curl. Installa il client curl per usare HTTP e HTTPS")?;
    let copied = std::io::copy(&mut child.stdout.take().unwrap().take(limit + 1), output);
    if copied.as_ref().map_or(true, |n| *n > limit) {
        let _ = child.kill();
    }
    let status = child.wait()?;
    ensure!(copied? <= limit, "Risposta HTTP oltre il limite");
    ensure!(status.success(), "Trasferimento curl fallito ({status})");
    let headers = std::fs::read(response_headers.path())?;
    let headers = String::from_utf8_lossy(&headers);
    let final_headers = headers
        .trim_end()
        .rsplit("\r\n\r\n")
        .next()
        .unwrap_or_default();
    let code = final_headers
        .lines()
        .next()
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    if !(200..300).contains(&code) {
        return Err(HttpError(code).into());
    }
    Ok(final_headers
        .lines()
        .filter_map(|s| s.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .and_then(|(_, v)| v.trim().split(';').next())
        .map(str::to_owned))
}
