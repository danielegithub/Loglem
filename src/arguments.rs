//! Small parser for the public CLI. OS paths remain native strings.
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::{HashMap, VecDeque},
    ffi::{OsStr, OsString},
    path::PathBuf,
};

pub(crate) trait Value: Sized {
    fn parse(s: OsString) -> Result<Self>;
}
impl Value for PathBuf {
    fn parse(s: OsString) -> Result<Self> {
        Ok(s.into())
    }
}
impl Value for String {
    fn parse(s: OsString) -> Result<Self> {
        s.into_string()
            .map_err(|_| anyhow::anyhow!("Valore non UTF-8"))
    }
}
macro_rules! numeric {
    ($($ty:ty),*) => {
        $(impl Value for $ty {
            fn parse(input:OsString)->Result<Self> {
                Ok(String::parse(input)?.parse()?)
            }
        })*
    }
}
numeric!(bool, u8, u16, u32, u64, usize, std::net::IpAddr);

macro_rules! enumeration {
    ($($ty:ty),*) => {
        $(impl Value for $ty {
            fn parse(input:OsString)->Result<Self> {
                let name=String::parse(input)?.replace('-',"_");
                Ok(serde_json::from_value(serde_json::Value::String(name))?)
            }
        })*
    }
}
enumeration!(
    crate::query::Level,
    crate::model::LogKind,
    crate::model::RetentionUnit,
    crate::access::Role
);
pub(crate) struct Options {
    options: HashMap<String, Vec<OsString>>,
    positions: VecDeque<OsString>,
}
impl Options {
    pub fn new(raw: Vec<OsString>, flags: &[&str], options: &[&str]) -> Result<Self> {
        let mut args = raw.into_iter();
        let mut parsed = Self {
            options: HashMap::new(),
            positions: VecDeque::new(),
        };
        while let Some(arg) = args.next() {
            if arg == "--" {
                parsed.positions.extend(args);
                break;
            }
            if let Some(opt) = arg.to_str().filter(|s| s.starts_with('-') && *s != "-") {
                let (name, value) = opt
                    .split_once('=')
                    .map_or((opt, None), |(k, v)| (k, Some(v)));
                if flags.contains(&name) {
                    ensure!(
                        value.is_none() && !parsed.options.contains_key(name),
                        "Opzione non valida: {opt}"
                    );
                    parsed.options.insert(name.into(), vec![]);
                } else if options.contains(&name) {
                    let value = match value {
                        Some(s) => s.into(),
                        None => args
                            .next()
                            .filter(|v| !v.to_string_lossy().starts_with('-'))
                            .with_context(|| format!("Manca il valore di {name}"))?,
                    };
                    parsed.options.entry(name.into()).or_default().push(value);
                } else {
                    bail!("Opzione sconosciuta: {name}");
                }
            } else {
                parsed.positions.push_back(arg);
            }
        }
        Ok(parsed)
    }
    pub fn flag(&mut self, k: &str) -> bool {
        self.options.remove(k).is_some()
    }
    pub fn option<T: Value>(&mut self, k: &str) -> Result<Option<T>> {
        let Some(v) = self.options.remove(k) else {
            return Ok(None);
        };
        ensure!(v.len() == 1, "Opzione ripetuta: {k}");
        v.into_iter().next().map(T::parse).transpose()
    }
    pub fn many<T: Value>(&mut self, k: &str) -> Result<Vec<T>> {
        self.options
            .remove(k)
            .unwrap_or_default()
            .into_iter()
            .map(T::parse)
            .collect()
    }
    pub fn pos<T: Value>(&mut self, _: &str) -> Result<Option<T>> {
        self.positions.pop_front().map(T::parse).transpose()
    }
    pub fn relationship(&self, key: &str, other: &str, required: bool) -> Result<()> {
        ensure!(
            !self.options.contains_key(key) || self.options.contains_key(other) == required,
            "Combinazione non valida di {key} e {other}"
        );
        Ok(())
    }
    pub fn finish(self) -> Result<()> {
        ensure!(
            self.positions.is_empty() && self.options.is_empty(),
            "Argomenti in eccesso"
        );
        Ok(())
    }
}
pub(crate) fn checked_limit(n: u64) -> Result<u16> {
    ensure!((1..=1000).contains(&n), "Il limite va da 1 a 1000");
    Ok(n as u16)
}
fn help(path: &str) -> Option<&'static str> {
    include_str!("cli_help.txt")
        .split("\n@@\n")
        .filter_map(|s| s.split_once('\n'))
        .find(|(k, _)| *k == path)
        .map(|(_, v)| v)
}
pub fn parse() -> Result<Option<crate::cli::Cli>> {
    let mut data_dir = std::env::var_os("LOGLEM_DATA_DIR").map(PathBuf::from);
    let mut args = std::env::args_os().skip(1);
    let mut raw = VecDeque::new();
    let mut end = false;
    let mut show = false;
    while let Some(s) = args.next() {
        if !end && s == "--data-dir" {
            data_dir = Some(args.next().context("Manca la cartella dati")?.into());
            continue;
        }
        if !end {
            if let Some(v) = s.to_str().and_then(|s| s.strip_prefix("--data-dir=")) {
                data_dir = Some(v.into());
                continue;
            }
        }
        if !end && (s == "--help" || s == "-h") {
            show = true;
            continue;
        }
        if s == "--" {
            end = true;
        }
        raw.push_back(s);
    }
    if raw.front().is_some_and(|s| s == "-V" || s == "--version") {
        println!("loglem {}", crate::VERSION);
        return Ok(None);
    }
    if raw.front().is_some_and(|s| s == "help") {
        raw.pop_front();
        show = true;
    }
    let mut path = String::new();
    if let Some(s) = raw.pop_front() {
        path = String::parse(s)?;
    }
    if matches!(
        path.as_str(),
        "site"
            | "source"
            | "proof"
            | "privacy"
            | "alerts"
            | "access"
            | "config"
            | "service"
            | "update"
    ) {
        if let Some(sub) = raw.pop_front() {
            if sub == OsStr::new("help") {
                show = true;
                if let Some(sub) = raw.pop_front() {
                    path.push(' ');
                    path.push_str(&String::parse(sub)?);
                }
            } else {
                path.push(' ');
                path.push_str(&String::parse(sub)?);
            }
        } else if !show {
            bail!("{}", help(&path).unwrap_or_default())
        }
    }
    if show {
        print!("{}", help(&path).context("Comando sconosciuto")?);
        return Ok(None);
    }
    let command = if path.is_empty() {
        None
    } else {
        Some(crate::cli::parse_command(&path, raw.into())?)
    };
    Ok(Some(crate::cli::Cli { data_dir, command }))
}
