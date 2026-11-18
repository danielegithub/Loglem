//! Posizioni configurate, file fisici e somiglianza fra le righe già raccolte.
mod external;

use crate::sources::retention::OwnerNote;
use crate::{
    archive::Archive,
    config,
    model::{Input, Source},
    sources::file,
    util,
};
use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const ROWS: usize = 200;
type Fingerprint = [u8; 32];
type Sample = Vec<Fingerprint>;

#[derive(Serialize)]
pub struct Location {
    kind: &'static str,
    path: String,
    bytes: Option<u64>,
    /// Chi lo cancella, in poche parole.
    owner: String,
    /// La spiegazione lunga e cosa fare (può mancare).
    help: String,
    /// `ok`, `info` o `warn`: il colore dell'etichetta.
    level: &'static str,
    // Local files can be verified. For a journal or runtime only the configured stream is known.
    state: &'static str,
}

#[derive(Serialize, Default)]
pub struct Copies {
    pub positions: usize,
    pub locations: Vec<Location>,
}

#[derive(Serialize)]
pub struct Similarity {
    pub source: String,
    pub other: String,
    pub common: usize,
    pub compared: usize,
    pub external: bool,
}

#[derive(Serialize, Default)]
pub struct Report {
    pub sources: BTreeMap<String, Copies>,
    pub original_files: usize,
    pub archives: usize,
    pub original_bytes: u64,
    pub incomplete_originals: bool,
    pub archive_bytes: u64,
    pub matches: Vec<Similarity>,
    pub unsampled: Vec<String>,
    pub external: Option<external::Coverage>,
}

pub(crate) fn file_key(path: &Path, meta: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        let _ = path;
        file::identity(meta)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        path.canonicalize()
            .unwrap_or_else(|_| path.into())
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Default)]
struct Inventory {
    originals: HashSet<String>,
    archives: HashSet<String>,
}

fn file_location(kind: &'static str, path: &Path, owner: &OwnerNote) -> (Location, Option<String>) {
    let metadata = fs::symlink_metadata(path);
    let missing = metadata
        .as_ref()
        .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound);
    let meta = metadata.ok().filter(|m| m.is_file());
    let key = meta.as_ref().map(|m| file_key(path, m));
    let bytes = meta.map(|m| m.len());
    (
        Location {
            kind,
            path: path.display().to_string(),
            bytes,
            owner: owner.text.clone(),
            help: owner.help.clone(),
            level: owner.level,
            state: if kind == "archive" && missing {
                "absent"
            } else if bytes.is_some() {
                "present"
            } else {
                "unavailable"
            },
        },
        key,
    )
}

fn configured_origin(input: &Input) -> Option<(String, OwnerNote)> {
    let runtime = |name: &str| {
        OwnerNote::new(
            format!("{name} · finché esiste il container"),
            "info",
            format!(
                "{name} tiene i log del container finché il container esiste. Per sapere se sono limitati in dimensione o in tempo guarda «Conservazione degli originali»."
            ),
        )
    };
    Some(match input {
        Input::Journal(filter) => (
            format!("Journal · {}", filter.describe()),
            OwnerNote::new(
                "journald · secondo la sua configurazione",
                "info",
                "Il journal di sistema decide da solo per quanto tenere i log. Guarda «Conservazione degli originali» per i suoi limiti.",
            ),
        ),
        Input::Podman {
            container,
            connection,
        } => (
            format!(
                "Podman · {} · {container}",
                connection.as_deref().unwrap_or("default")
            ),
            runtime("Podman"),
        ),
        Input::Docker { container, context } => (
            format!(
                "Docker · {} · {container}",
                context.as_deref().unwrap_or("default")
            ),
            runtime("Docker"),
        ),
        Input::Kubernetes(target) => (
            format!(
                "{} · {}",
                target.context.as_deref().unwrap_or("default"),
                target.describe()
            ),
            OwnerNote::new(
                "Il nodo · secondo il suo runtime",
                "info",
                "I log dei pod li tiene il nodo Kubernetes, secondo la configurazione del suo runtime.",
            ),
        ),
        _ => return None,
    })
}

// Other rotation schemes may be visible on disk even when collection has no explicit glob.
fn rotated_name(active: &str, name: &str) -> bool {
    let plain = name.strip_suffix(".gz").unwrap_or(name);
    if name != active && plain == active {
        return true;
    }
    let dated = |v: &str| {
        chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok()
            || (v.len() == 8 && chrono::NaiveDate::parse_from_str(v, "%Y%m%d").is_ok())
            || chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H-%M-%S%.f").is_ok()
    };
    if let Some(suffix) = plain
        .strip_prefix(active)
        .and_then(|s| s.strip_prefix(['.', '-']))
    {
        return (!suffix.is_empty()
            && suffix.len() <= 5
            && suffix.bytes().all(|b| b.is_ascii_digit()))
            || dated(suffix);
    }
    active
        .strip_suffix(".log")
        .and_then(|stem| plain.strip_prefix(stem))
        .and_then(|v| v.strip_prefix('-'))
        .and_then(|v| v.strip_suffix(".log"))
        .is_some_and(dated)
}

pub(crate) fn visible_rotations(active: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if let (Some(root), Some(base)) = (active.parent(), active.file_name().and_then(|n| n.to_str()))
    {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if rotated_name(base, &entry.file_name().to_string_lossy())
                && entry.file_type()?.is_file()
            {
                found.push(entry.path());
            }
        }
    }
    Ok(found)
}

// Folders collect .log and .log.gz inputs. Also show the rotated siblings that still occupy disk.
fn folder_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    if name.ends_with(".log.gz") {
        return Some("rotated");
    }
    if let Some(pos) = name.rfind(".log") {
        let base = &name[..pos + 4];
        if rotated_name(base, name) {
            return Some("rotated");
        }
    }
    name.ends_with(".log").then_some("original")
}

fn visible_in_folder(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if folder_kind(&path).is_some() && entry.file_type()?.is_file() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn inventory(dir: &Path, source: &Source, seen: &mut Inventory, report: &mut Report) {
    let mut copies = Copies::default();
    let mut paths = BTreeMap::<PathBuf, (&str, Option<OwnerNote>)>::new();
    // `None` = chi ripulisce il file si decide dopo, leggendo logrotate una volta sola.
    let producer: Option<OwnerNote> = None;
    let cleanup = Some(OwnerNote::new(
        "Loglem · dopo 24 ore ferme",
        "ok",
        "Loglem lo cancella dopo averlo archiviato e controllato riga per riga, quando resta fermo da 24 ore.",
    ));
    let rotation_rules = crate::logrotate::Rules::load();
    let mut folder = |root: &Path,
                      files: Result<Vec<PathBuf>>,
                      kind: &'static str,
                      owner: Option<OwnerNote>| match files {
        Ok(files) => {
            for path in files {
                let kind = if kind == "original" {
                    folder_kind(&path).unwrap_or(kind)
                } else {
                    kind
                };
                paths.insert(path, (kind, owner.clone()));
            }
        }
        Err(_) => {
            report.incomplete_originals = true;
            let note = owner.unwrap_or_else(|| {
                OwnerNote::new(
                    "Non leggibile",
                    "warn",
                    "Loglem non riesce a leggere questa cartella: controlla i permessi e il percorso.",
                )
            });
            copies.locations.push(Location {
                kind: "folder",
                path: root.display().to_string(),
                bytes: None,
                owner: note.text,
                help: note.help,
                level: note.level,
                state: "unavailable",
            });
        }
    };
    if let Some(root) = &source.completed_dir {
        folder(root, file::folder_files(root), "completed", cleanup.clone());
    }
    match &source.input {
        Input::Folder { path, .. } => {
            folder(path, visible_in_folder(path), "original", producer.clone())
        }
        Input::File {
            path,
            rotated_pattern,
        } => {
            let parent = path.parent().unwrap_or(path);
            folder(parent, visible_rotations(path), "rotated", producer.clone());
            folder(
                parent,
                file::candidates(path, rotated_pattern.as_deref()),
                "rotated",
                if source.remove_rotated {
                    cleanup.clone()
                } else {
                    producer.clone()
                },
            );
            paths.insert(path.clone(), ("original", producer));
        }
        input => {
            if let Some((path, note)) = configured_origin(input) {
                copies.locations.push(Location {
                    kind: if matches!(input, Input::Journal(_)) {
                        "journal"
                    } else {
                        "runtime"
                    },
                    path,
                    bytes: None,
                    state: "configured",
                    owner: note.text,
                    help: note.help,
                    level: note.level,
                });
                copies.positions += 1;
                report.incomplete_originals = true;
            }
        }
    }
    let mut source_keys = HashSet::new();
    for (path, (kind, owner)) in paths {
        let owner = owner.unwrap_or_else(|| {
            crate::sources::retention::cleanup_owner(
                rotation_rules.as_ref(),
                &path,
                kind == "rotated",
            )
        });
        let (location, key) = file_location(kind, &path, &owner);
        if let Some(key) = key {
            if seen.originals.insert(key.clone()) {
                report.original_bytes += location.bytes.unwrap_or_default();
            }
            if source_keys.insert(key) {
                copies.positions += 1;
            }
        } else {
            report.incomplete_originals = true;
        }
        copies.locations.push(location);
    }
    let path = source.archive_path(dir);
    let (mut archive, key) = file_location(
        "archive",
        &path,
        &OwnerNote::new(
            format!("Loglem · dopo {}", source.retention),
            "ok",
            "Loglem cancella la sua copia passato questo tempo. I file originali non li tocca.",
        ),
    );
    if let Some(key) = key {
        let bytes = util::bytes_on_disk(&path);
        archive.bytes = Some(bytes);
        if seen.archives.insert(key) {
            report.archive_bytes += bytes;
        }
        copies.positions += 1;
    }
    copies.locations.push(archive);
    report.sources.insert(source.id.clone(), copies);
}

// Classic and RFC3339 syslog headers. Preserve application text that merely contains ": ".
fn syslog_payload(line: &str) -> &str {
    let mut words = line.split_ascii_whitespace();
    let first = words.next().unwrap_or_default();
    let date = if [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .contains(&first)
    {
        words
            .next()
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|d| (1..=31).contains(&d))
            && words
                .next()
                .is_some_and(|s| chrono::NaiveTime::parse_from_str(s, "%H:%M:%S").is_ok())
    } else {
        chrono::DateTime::parse_from_rfc3339(first).is_ok()
    };
    if date && words.next().is_some() {
        if let Some(tag) = words.next().filter(|s| s.ends_with(':')) {
            if let Some((_, text)) = line.split_once(&format!(" {tag} ")) {
                return text;
            }
        }
    }
    line
}

fn fingerprint(line: &str, input: Option<&Input>) -> Option<Fingerprint> {
    let text = match input {
        Some(Input::Journal(_)) => line.split_once(": ").map_or(line, |(_, s)| s),
        Some(Input::Kubernetes(t)) if t.selector.is_some() => line
            .strip_prefix("[pod/")
            .and_then(|s| s.split_once("] "))
            .map_or(line, |(_, s)| s),
        _ => line,
    };
    // File-backed containers may store the Docker JSON or CRI envelope verbatim.
    #[derive(serde::Deserialize)]
    struct DockerLine<'a> {
        #[serde(borrow)]
        log: std::borrow::Cow<'a, str>,
        stream: &'a str,
        time: &'a str,
    }
    let envelope = text
        .starts_with('{')
        .then(|| serde_json::from_str::<DockerLine>(text).ok())
        .flatten();
    let text = if let Some(record) = envelope.as_ref().filter(|r| {
        matches!(r.stream, "stdout" | "stderr")
            && chrono::DateTime::parse_from_rfc3339(r.time).is_ok()
    }) {
        record.log.as_ref()
    } else if let Some((stamp, rest)) = text.split_once(' ') {
        rest.strip_prefix("stdout F ")
            .or_else(|| rest.strip_prefix("stderr F "))
            .filter(|_| chrono::DateTime::parse_from_rfc3339(stamp).is_ok())
            .unwrap_or(text)
    } else {
        text
    };
    let text = syslog_payload(text.trim_end_matches(['\r', '\n']));
    if text.trim().is_empty() {
        return None;
    }
    Some(Sha256::digest(text.as_bytes()).into())
}

fn archive_sample(dir: &Path, source: &Source) -> Result<Sample> {
    let db = Archive::open_reader(&source.archive_path(dir))?;
    let mut stmt = db
        .db
        .prepare("SELECT message FROM log_rows ORDER BY id DESC LIMIT ?1")?;
    let mut rows = stmt.query([ROWS as i64])?;
    let mut sample = Sample::new();
    while let Some(row) = rows.next()? {
        let value = row.get_ref(0)?.as_str()?;
        sample.extend(fingerprint(value, Some(&source.input)));
    }
    sample.sort_unstable();
    Ok(sample)
}

fn compare(
    source: &str,
    a: &Sample,
    other: &str,
    b: &Sample,
    external: bool,
) -> Option<Similarity> {
    let mut i = 0;
    let mut j = 0;
    let mut common = 0;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let compared = a.len().max(b.len());
    (common * 2 > compared).then(|| Similarity {
        source: source.into(),
        other: other.into(),
        common,
        compared,
        external,
    })
}

pub fn report(dir: &Path, site: &str, external: bool) -> Result<Report> {
    let _access = util::DataAccess::read(dir)?;
    let cfg = config::load(dir)?;
    ensure!(
        site == "~" || cfg.sites.contains_key(site),
        "Sito inesistente"
    );
    let id = if site == "~" { "" } else { site };
    let mut report = Report::default();
    let mut seen = Inventory::default();
    let mut samples: Vec<(&Source, Sample)> = Vec::new();
    for s in cfg.sources.iter().filter(|s| s.site == id) {
        inventory(dir, s, &mut seen, &mut report);
        match archive_sample(dir, s) {
            Ok(sample) if !sample.is_empty() => {
                for (other, lines) in &samples {
                    report
                        .matches
                        .extend(compare(&s.id, &sample, &other.id, lines, false));
                }
                samples.push((s, sample));
            }
            _ => report.unsampled.push(s.id.clone()),
        }
    }
    report.original_files = seen.originals.len();
    report.archives = seen.archives.len();
    if external && !report.sources.is_empty() {
        let _lock = util::Lock::acquire(&dir.join("duplicate-scan.lock"))?;
        let result = external::scan(
            dir,
            &cfg.sources,
            &samples,
            &seen.originals,
            &mut report.matches,
            &report.sources,
        );
        report.external = Some(result);
    }
    Ok(report)
}

/// Recheck both selections under the configuration lock: two opposite clicks can only pause one.
pub fn pause(dir: &Path, site: &str, name: &str, keep: &str) -> Result<()> {
    ensure!(name != keep, "Scegli due sorgenti distinte");
    let _access = util::DataAccess::read(dir)?;
    let id = if site == "~" { "" } else { site };
    config::edit(dir, |cfg| {
        ensure!(
            cfg.sources
                .iter()
                .any(|s| s.id == keep && s.site == id && s.enabled && !s.detached),
            "La sorgente da mantenere non è più attiva in questo sito. Aggiorna il conteggio."
        );
        let s = cfg
            .sources
            .iter_mut()
            .find(|s| s.id == name && s.site == id && !s.detached)
            .ok_or_else(|| anyhow::anyhow!("Sorgente non più collegata al sito"))?;
        s.enabled = false;
        Ok(())
    })
}

impl Report {
    pub fn description(&self) -> String {
        let mut lines = vec![format!(
            "Originali misurati: {} B · Archivi: {} B",
            self.original_bytes, self.archive_bytes
        )];
        if self.incomplete_originals {
            lines.push("Alcune origini hanno dimensione sconosciuta.".into());
        }
        for (id, group) in &self.sources {
            lines.push(format!("{id} · {} posizioni", group.positions));
            for loc in &group.locations {
                let size = loc
                    .bytes
                    .map_or_else(|| "dimensione non disponibile".into(), |n| format!("{n} B"));
                lines.push(format!("  {} · {size} · {}", loc.path, loc.owner));
            }
        }
        if self.matches.is_empty() {
            lines.push("Nessun doppione nei campioni confrontati.".into());
        }
        for m in &self.matches {
            lines.push(format!(
                "{} ↔ {} · {} righe uguali su {}",
                m.source, m.other, m.common, m.compared
            ));
        }
        if !self.unsampled.is_empty() {
            lines.push(format!("Senza campione: {}", self.unsampled.join(", ")));
        }
        if let Some(scan) = &self.external {
            lines.push(format!(
                "Controllati {} file esterni e {} flussi · {} B di contenuto identico",
                scan.files, scan.streams, scan.identical_bytes
            ));
            for group in &scan.identical {
                for p in &group.paths {
                    lines.push(format!("  {} B · {}", group.bytes_each, p.display()));
                }
            }
            if scan.limited {
                lines.push("Raggiunto un limite della ricerca.".into());
            }
            for origin in &scan.unavailable {
                lines.push(format!("Non verificato: {origin}"));
            }
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(lines: &[&str]) -> Sample {
        let mut result: Sample = lines.iter().filter_map(|s| fingerprint(s, None)).collect();
        result.sort_unstable();
        result
    }

    #[test]
    fn compares_occurrences_and_requires_strict_majority_of_larger_sample() {
        let a = sample(&["a", "a", "b", "c"]);
        assert!(compare("a", &a, "b", &sample(&["a", "b", "z", "z"]), false).is_none());
        assert!(compare("a", &a, "b", &sample(&["a"]), false).is_none());
        let hit = compare("a", &a, "b", &sample(&["a", "a", "a", "b"]), false).unwrap();
        assert_eq!((hit.common, hit.compared), (3, 4));
        assert!(compare("a", &vec![], "b", &vec![], false).is_none());
    }

    #[test]
    fn recognizes_real_envelopes_and_preserves_application_text() {
        let plain = fingerprint("access: event 42", None);
        assert_eq!(
            plain,
            fingerprint(
                "web[7]: access: event 42",
                Some(&Input::Journal(Default::default()))
            )
        );
        assert_eq!(
            plain,
            fingerprint("Nov  3 10:04:05 host web[7]: access: event 42", None)
        );
        assert_eq!(
            plain,
            fingerprint("2026-10-03T10:04:05Z host web: access: event 42", None)
        );
        assert_eq!(
            plain,
            fingerprint(
                r#"{"log":"access: event 42\n","stream":"stdout","time":"2026-10-03T10:04:05Z"}"#,
                None
            )
        );
        assert_eq!(
            plain,
            fingerprint("2026-10-03T10:04:05Z stderr F access: event 42", None)
        );
        assert_ne!(plain, fingerprint("event 42", None));
        assert_eq!(None, fingerprint("  \n", None));
    }

    #[test]
    fn counts_shared_files_once_and_keeps_sites_separate() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let data = tmp.path().join("data");
        let file = tmp.path().join("web.log");
        let alias = tmp.path().join("alias.log");
        fs::write(&file, "one\ntwo\n")?;
        fs::hard_link(&file, &alias)?;
        fs::write(tmp.path().join("web.log.1"), "old")?;
        fs::write(tmp.path().join("web.log.other"), "unrelated")?;
        let site = config::add_site(&data, "example.com")?;
        let other = config::add_site(&data, "other.example")?;
        for (id, path, site) in [
            ("first", &file, &site),
            ("alias", &alias, &site),
            ("other", &file, &other),
        ] {
            let s = Source {
                id: id.into(),
                site: site.clone(),
                input: Input::File {
                    path: path.clone(),
                    rotated_pattern: None,
                },
                ..Default::default()
            };
            config::add(&data, s.clone())?;
            let mut store = Archive::open(&s.archive_path(&data))?;
            file::collect(&mut store, path, false, crate::timestamps::now_ns())?;
        }
        let r = report(&data, &site, false)?;
        assert_eq!(r.sources.len(), 2);
        assert_eq!(r.original_bytes, 11);
        assert!(!r.incomplete_originals);
        assert_eq!(r.matches.len(), 1);
        assert_eq!((r.matches[0].common, r.matches[0].compared), (2, 2));
        assert_eq!(r.sources["first"].positions, 3);
        assert_eq!(r.sources["alias"].positions, 2);
        assert!(r.external.is_none());
        assert!(r.archive_bytes > 0);
        assert!(pause(&data, &site, "alias", "other").is_err());
        pause(&data, &site, "alias", "first")?;
        pause(&data, &site, "alias", "first")?;
        assert!(pause(&data, &site, "first", "alias").is_err());
        assert!(config::source(&data, "first")?.enabled);
        Ok(())
    }

    #[test]
    fn samples_only_the_last_200_and_decodes_compressed_archive_rows() -> Result<()> {
        use std::io::Write;
        let tmp = tempfile::tempdir()?;
        let s = Source {
            id: "archive".into(),
            ..Default::default()
        };
        let db = Archive::open(&s.archive_path(tmp.path()))?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"recent")?;
        let bytes = encoder.finish()?;
        for i in 0..210 {
            db.db.execute("INSERT INTO entries(record_key,generation,byte_offset,event_ns,collected_ns,timestamp_origin,message) VALUES(?1,0,0,0,0,'event',?2)", rusqlite::params![i, if i >= 10 { rusqlite::types::Value::Blob(bytes.clone()) } else { "old".to_string().into() }])?;
        }
        let recent = archive_sample(tmp.path(), &s)?;
        assert_eq!(recent, vec![fingerprint("recent", None).unwrap(); ROWS]);
        Ok(())
    }
}
