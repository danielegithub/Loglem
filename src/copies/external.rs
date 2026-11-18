//! Ricerca esplicita e limitata alle sorgenti locali. Nessun testo dei log esce dal confronto.
use super::{compare, file_key, fingerprint, Fingerprint, Sample, Similarity, ROWS};
use crate::{
    model::{Input, Source},
    process,
    sources::file,
};
use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const WINDOW: u64 = 256 * 1024;
const VERIFY_BYTES: u64 = 256 * 1024 * 1024;
const FILES: usize = 96;
const STREAMS: usize = 12;
const DURATION: Duration = Duration::from_secs(8);

#[derive(Default, Serialize)]
pub struct Coverage {
    pub files: usize,
    pub streams: usize,
    pub limited: bool,
    pub unavailable: BTreeSet<String>,
    pub identical: Vec<Identical>,
    pub identical_bytes: u64,
}

#[derive(Serialize)]
pub struct Identical {
    pub paths: Vec<PathBuf>,
    pub bytes_each: u64,
}

struct FileSample {
    sample: Sample,
    tail_hash: (Fingerprint, u64),
    partial: bool,
    key: String,
}

fn open_regular(path: &Path) -> Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = opts.open(path)?;
    ensure!(f.metadata()?.is_file(), "File non regolare");
    Ok(f)
}

fn sample_file(path: &Path) -> Result<FileSample> {
    let mut f = open_regular(path)?;
    let meta = f.metadata()?;
    let len = meta.len();
    let start = len.saturating_sub(WINDOW);
    f.seek(SeekFrom::Start(start))?;
    let mut data = Vec::new();
    (&mut f).take(WINDOW).read_to_end(&mut data)?;
    let after = f.metadata()?;
    ensure!(
        after.len() == len
            && meta.modified()? == after.modified()?
            && data.len() as u64 == len - start,
        "File in modifica"
    );
    let tail_hash = (Sha256::digest(&data).into(), len);
    let mut partial = false;
    if crate::sources::gzip::is_gzip(path) {
        let mut decoded = Vec::new();
        if start == 0 {
            flate2::read::MultiGzDecoder::new(data.as_slice())
                .take(WINDOW + 1)
                .read_to_end(&mut decoded)?;
        }
        partial = start > 0 || decoded.len() as u64 > WINDOW;
        data = if partial { Vec::new() } else { decoded };
    }
    let tail = if start == 0 {
        &data[..]
    } else {
        data.splitn(2, |b| *b == b'\n').nth(1).unwrap_or_default()
    };
    let mut sample = Vec::new();
    if let Some(end) = tail.iter().rposition(|b| *b == b'\n') {
        for line in tail[..end].rsplit(|b| *b == b'\n').take(ROWS) {
            sample.extend(fingerprint(std::str::from_utf8(line)?, None));
        }
    }
    sample.sort_unstable();
    Ok(FileSample {
        sample,
        tail_hash,
        partial,
        key: file_key(path, &meta),
    })
}

struct Scan<'a> {
    deadline: Instant,
    samples: &'a [(&'a Source, Sample)],
    matches: &'a mut Vec<Similarity>,
    coverage: Coverage,
}

impl Scan<'_> {
    fn remaining(&mut self) -> Option<Duration> {
        let rem = self.deadline.saturating_duration_since(Instant::now());
        if rem.is_zero() {
            self.coverage.limited = true;
            None
        } else {
            Some(rem.min(Duration::from_millis(1200)))
        }
    }

    fn compare(&mut self, label: &str, sample: &Sample, skip: impl Fn(&Source) -> bool) {
        for (source, lines) in self.samples {
            if !skip(source) {
                self.matches
                    .extend(compare(&source.id, lines, label, sample, true));
            }
        }
    }

    fn command(
        &mut self,
        name: &str,
        cmd: &mut Command,
        stderr_logs: bool,
        mut consume: impl FnMut(&str),
    ) -> bool {
        let Some(timeout) = self.remaining() else {
            return false;
        };
        let mut bytes = 0;
        let result = process::stream(cmd, timeout, |err, line| {
            bytes += line.len();
            ensure!(bytes <= 2 * 1024 * 1024, "Limite di lettura");
            if !err || stderr_logs {
                consume(std::str::from_utf8(&line)?.trim_end_matches(['\r', '\n']));
            }
            Ok(())
        });
        if result.is_ok_and(|s| s.success()) {
            true
        } else {
            self.coverage.unavailable.insert(name.to_owned());
            false
        }
    }

    fn journal(&mut self, sources: &[Source], journal_containers: &mut HashSet<String>) {
        let mut selectors = BTreeSet::new();
        let mut cmd = Command::new("journalctl");
        cmd.args([
            "--no-pager",
            "--quiet",
            "-n",
            "200",
            "-o",
            "json",
            "--output-fields=_SYSTEMD_UNIT,CONTAINER_ID_FULL,CONTAINER_NAME,SYSLOG_IDENTIFIER",
        ]);
        if !self.command("Journal", &mut cmd, false, |line| {
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                return;
            };
            for key in [
                "CONTAINER_ID_FULL",
                "CONTAINER_NAME",
                "SYSLOG_IDENTIFIER",
                "_SYSTEMD_UNIT",
            ] {
                if let Some(value) = row[key]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                {
                    selectors.insert(format!("{key}={value}"));
                    break;
                }
            }
        }) {
            return;
        }
        self.coverage.limited |= selectors.len() > STREAMS;
        for selector in selectors.into_iter().take(STREAMS) {
            if self.remaining().is_none() {
                break;
            }
            let Some((field, value)) = selector.split_once('=') else {
                continue;
            };
            let is_container = matches!(field, "CONTAINER_ID_FULL" | "CONTAINER_NAME");
            if sources.iter().any(|s| {
                let Input::Journal(f) = &s.input else {
                    return false;
                };
                f.priority.is_none()
                    && f.matches.iter().all(|m| m == &selector)
                    && (f.units.is_empty()
                        || (field == "_SYSTEMD_UNIT" && f.units.iter().any(|u| u == value)))
                    && (f.identifiers.is_empty()
                        || (field == "SYSLOG_IDENTIFIER"
                            && f.identifiers.iter().any(|i| i == value)))
            }) {
                if is_container {
                    journal_containers.insert(value.to_owned());
                }
                continue;
            }
            let mut cmd = Command::new("journalctl");
            cmd.args([
                "--no-pager",
                "--quiet",
                "-n",
                "200",
                "-o",
                "json",
                "--output-fields=MESSAGE",
                &selector,
            ]);
            let mut sample = Sample::new();
            if self.command("Journal", &mut cmd, false, |line| {
                if let Ok(row) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(s) = row["MESSAGE"].as_str() {
                        if sample.len() < ROWS {
                            sample.extend(fingerprint(s, None));
                        }
                    }
                }
            }) {
                sample.sort_unstable();
                self.coverage.streams += 1;
                if is_container && !sample.is_empty() {
                    journal_containers.insert(value.to_owned());
                }
                self.compare(&format!("Journal · {selector}"), &sample, |s| {
                    match &s.input {
                        Input::Journal(_) => true,
                        Input::Docker {
                            container,
                            context: None,
                        }
                        | Input::Podman {
                            container,
                            connection: None,
                        } => field == "CONTAINER_ID_FULL" && container == value,
                        _ => false,
                    }
                });
            }
        }
    }

    fn runtime(&mut self, name: &str, sources: &[Source], journal_containers: &HashSet<String>) {
        let local = || {
            let mut cmd = Command::new(name);
            if name == "docker" {
                let socket = std::env::var("DOCKER_HOST")
                    .ok()
                    .filter(|s| s.starts_with("unix:///"))
                    .unwrap_or_else(|| "unix:///var/run/docker.sock".into());
                cmd.env_remove("DOCKER_CONTEXT").arg("--host").arg(socket);
            } else {
                cmd.env_remove("CONTAINER_HOST")
                    .env_remove("CONTAINER_CONNECTION")
                    .arg("--remote=false");
            }
            cmd
        };
        let mut ids = Vec::new();
        let mut truncated = false;
        if !self.command(
            name,
            local().args(["ps", "-a", "--no-trunc", "--format", "{{.ID}} {{.Names}}"]),
            false,
            |line| {
                if let Some((id, label)) = line.split_once(' ') {
                    if id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                        if ids.len() < STREAMS {
                            ids.push((id.to_owned(), label.to_owned()));
                        } else {
                            truncated = true;
                        }
                    }
                }
            },
        ) {
            return;
        }
        self.coverage.limited |= truncated;
        for (id, label) in ids {
            if sources.iter().any(|s| match &s.input {
                Input::Docker {
                    container,
                    context: None,
                } if name == "docker" => *container == id,
                Input::Podman {
                    container,
                    connection: None,
                } if name == "podman" => *container == id,
                _ => false,
            }) {
                continue;
            }
            if self.remaining().is_none() {
                break;
            }
            let mut driver = String::new();
            let journal_source = self
                .samples
                .iter()
                .any(|(s, _)| matches!(s.input, Input::Journal(_)));
            let same_container =
                journal_containers.contains(&id) || journal_containers.contains(&label);
            if journal_source || same_container {
                self.command(
                    name,
                    local().args(["inspect", "--format", "{{.HostConfig.LogConfig.Type}}", &id]),
                    false,
                    |line| driver = line.to_owned(),
                );
            }
            if same_container && driver == "journald" {
                continue;
            }
            let mut sample = Sample::new();
            if self.command(
                name,
                local().args(["logs", "--tail", "200", "--timestamps", &id]),
                true,
                |line| {
                    if let Some((date, text)) = line.split_once(' ') {
                        if sample.len() < ROWS && chrono::DateTime::parse_from_rfc3339(date).is_ok()
                        {
                            sample.extend(fingerprint(text, None));
                        }
                    }
                },
            ) {
                self.coverage.streams += 1;
                sample.sort_unstable();
                self.compare(
                    &format!("{name} · {label} ({})", &id[..12]),
                    &sample,
                    |s| driver == "journald" && matches!(s.input, Input::Journal(_)),
                );
            }
        }
    }
}

struct Candidate {
    path: PathBuf,
    key: String,
    managed: bool,
}

type FileGroups = BTreeMap<(u64, Fingerprint), Vec<Candidate>>;

fn full_digest(file: &Candidate, len: u64, deadline: Instant) -> Result<Fingerprint> {
    let mut handle = open_regular(&file.path)?;
    let before = handle.metadata()?;
    ensure!(
        before.len() == len && file_key(&file.path, &before) == file.key,
        "File cambiato"
    );
    let mut remaining = len;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        ensure!(Instant::now() < deadline, "Tempo esaurito");
        let size = remaining.min(buffer.len() as u64) as usize;
        handle.read_exact(&mut buffer[..size])?;
        hash.update(&buffer[..size]);
        remaining -= size as u64;
    }
    let after = handle.metadata()?;
    ensure!(
        after.len() == len && before.modified()? == after.modified()?,
        "File in modifica"
    );
    Ok(hash.finalize().into())
}

fn exact_groups(scan: &mut Scan, groups: FileGroups) {
    let mut budget = VERIFY_BYTES;
    for ((bytes, tail), group) in groups {
        if group.len() < 2 || !group.iter().any(|f| f.managed) {
            continue;
        }
        let mut verified = BTreeMap::<Fingerprint, Vec<Candidate>>::new();
        for candidate in group {
            let hash = if bytes <= WINDOW {
                tail
            } else {
                if bytes > budget || scan.remaining().is_none() {
                    scan.coverage.limited = true;
                    break;
                }
                budget -= bytes;
                match full_digest(&candidate, bytes, scan.deadline) {
                    Ok(hash) => hash,
                    Err(_) => {
                        scan.coverage
                            .unavailable
                            .insert("Verifica di alcuni file".into());
                        continue;
                    }
                }
            };
            verified.entry(hash).or_default().push(candidate);
        }
        for identical in verified
            .into_values()
            .filter(|g| g.len() > 1 && g.iter().any(|f| f.managed))
        {
            let mut paths: Vec<_> = identical.into_iter().map(|f| f.path).collect();
            paths.sort();
            scan.coverage.identical_bytes += bytes * (paths.len() as u64 - 1);
            scan.coverage.identical.push(Identical {
                paths,
                bytes_each: bytes,
            });
        }
    }
    scan.coverage
        .identical
        .sort_by(|a, b| a.paths.cmp(&b.paths));
}

fn scan_files(
    scan: &mut Scan,
    known: &HashSet<String>,
    managed: &[PathBuf],
    roots: Vec<PathBuf>,
    data: &Path,
    other_sites: &[&Source],
) {
    let mut seen = HashSet::new();
    let mut groups = FileGroups::new();
    let mut managed_count = 0;
    let mut external_count = 0;
    let mut visit = |scan: &mut Scan, path: &Path, managed: bool| {
        let count = if managed {
            &mut managed_count
        } else {
            &mut external_count
        };
        if *count >= FILES || scan.remaining().is_none() {
            scan.coverage.limited = true;
            return false;
        }
        let Ok(meta) = fs::symlink_metadata(path) else {
            scan.coverage.unavailable.insert("Alcuni file".into());
            return true;
        };
        if !meta.is_file() {
            return true;
        }
        let key = file_key(path, &meta);
        if seen.contains(&key) || (!managed && known.contains(&key)) {
            return true;
        }
        *count += 1;
        match sample_file(path) {
            Ok(file) if file.key == key => {
                seen.insert(key.clone());
                let (tail, bytes) = file.tail_hash;
                if bytes > 0 {
                    groups.entry((bytes, tail)).or_default().push(Candidate {
                        path: path.into(),
                        key,
                        managed,
                    });
                }
                scan.coverage.limited |= file.partial;
                if !managed {
                    scan.coverage.files += 1;
                    scan.compare(&path.display().to_string(), &file.sample, |_| false);
                }
            }
            _ => {
                scan.coverage.unavailable.insert("Alcuni file".into());
            }
        }
        true
    };
    for path in managed {
        if !visit(scan, path, true) {
            break;
        }
    }
    let mut dirs: VecDeque<_> = roots.into_iter().map(|p| (p, 0)).collect();
    let mut scanned_dirs = HashSet::new();
    let mut entries = 0;
    let system_logs = Path::new("/var/log").canonicalize().ok();
    let service_logs = Path::new("/srv").canonicalize().ok();
    'dirs: while let Some((dir, depth)) = dirs.pop_front() {
        if scan.remaining().is_none() {
            break;
        }
        let Ok(dir) = dir.canonicalize() else {
            continue;
        };
        if dir.starts_with(data) || !scanned_dirs.insert(dir.clone()) {
            continue;
        }
        let Ok(children) = fs::read_dir(&dir) else {
            scan.coverage.unavailable.insert("Alcune cartelle".into());
            continue;
        };
        for child in children.flatten() {
            entries += 1;
            if entries > 4096 || scan.remaining().is_none() {
                scan.coverage.limited = true;
                break 'dirs;
            }
            let path = child.path();
            let Ok(kind) = child.file_type() else {
                continue;
            };
            let descend = (depth == 0 && Some(dir.as_path()) == system_logs.as_deref())
                || (depth < 3
                    && service_logs
                        .as_ref()
                        .is_some_and(|root| dir.starts_with(root)));
            if kind.is_dir() && descend {
                dirs.push_back((path, depth + 1));
            } else if kind.is_file()
                && file::log_name(&path)
                && !other_sites.iter().any(|s| reads_path(s, &path))
                && !visit(scan, &path, false)
            {
                break 'dirs;
            }
        }
    }
    exact_groups(scan, groups);
}

// A registered input for a different site is outside this search.
fn reads_path(s: &Source, path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let in_dir = |p: &Path| p.canonicalize().ok().as_deref() == path.parent();
    if s.completed_dir.as_deref().is_some_and(in_dir) && name.ends_with(".log") {
        return true;
    }
    match &s.input {
        Input::Folder { path: dir, .. } => in_dir(dir) && super::folder_kind(path).is_some(),
        Input::File {
            path: active,
            rotated_pattern,
        } if active.parent().is_some_and(in_dir) => {
            let base = active
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            name == base
                || super::rotated_name(base, name)
                || rotated_pattern
                    .as_ref()
                    .and_then(|p| glob::Pattern::new(p).ok())
                    .is_some_and(|p| {
                        p.matches(name) || p.matches(name.strip_suffix(".gz").unwrap_or(name))
                    })
        }
        _ => false,
    }
}

pub(super) fn scan(
    dir: &Path,
    sources: &[Source],
    samples: &[(&Source, Sample)],
    known: &HashSet<String>,
    matches: &mut Vec<Similarity>,
    copies: &BTreeMap<String, super::Copies>,
) -> Coverage {
    let mut roots = BTreeSet::new();
    let mut managed = Vec::new();
    for group in copies.values() {
        for loc in &group.locations {
            if matches!(loc.kind, "original" | "completed" | "rotated") && loc.state == "present" {
                let path = PathBuf::from(&loc.path);
                if let Some(parent) = path.parent() {
                    roots.insert(parent.to_owned());
                }
                managed.push(path);
            }
        }
    }
    let mut roots: Vec<_> = roots.into_iter().collect();
    roots.extend([PathBuf::from("/var/log"), PathBuf::from("/srv")]);
    let mut scan = Scan {
        deadline: Instant::now() + DURATION,
        samples,
        matches,
        coverage: Coverage::default(),
    };
    let other_sites: Vec<_> = sources
        .iter()
        .filter(|s| !copies.contains_key(&s.id))
        .collect();
    scan_files(
        &mut scan,
        known,
        &managed,
        roots,
        &dir.canonicalize().unwrap_or_else(|_| dir.to_owned()),
        &other_sites,
    );
    if !samples.is_empty() {
        let mut journal_containers = HashSet::new();
        scan.journal(sources, &mut journal_containers);
        for name in ["podman", "docker"] {
            scan.runtime(name, sources, &journal_containers);
        }
    }
    scan.coverage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_groups_ignore_aliases_and_similarity_alone_has_no_byte_value() -> Result<()> {
        let root = tempfile::tempdir()?;
        let active = root.path().join("source.log");
        let duplicate = root.path().join("copy.log");
        let content = "event 1\nevent 2\nevent 3\n";
        fs::write(&active, content)?;
        fs::write(&duplicate, content)?;
        fs::hard_link(&active, root.path().join("link.log"))?;
        fs::write(root.path().join("similar.log"), "event 1\nevent 2\nother\n")?;
        fs::write(root.path().join("other-site.log"), content)?;
        let source = Source {
            id: "events".into(),
            ..Default::default()
        };
        let other = Source {
            input: Input::File {
                path: root.path().join("other-site.log"),
                rotated_pattern: None,
            },
            ..Default::default()
        };
        let samples = [(&source, sample_file(&active)?.sample)];
        let mut matches = Vec::new();
        let mut scan = Scan {
            deadline: Instant::now() + DURATION,
            samples: &samples,
            matches: &mut matches,
            coverage: Coverage::default(),
        };
        scan_files(
            &mut scan,
            &HashSet::new(),
            std::slice::from_ref(&active),
            vec![root.path().into()],
            &root.path().join("data"),
            &[&other],
        );
        assert_eq!(scan.matches.len(), 2);
        assert_eq!(scan.coverage.files, 2);
        assert_eq!(scan.coverage.identical.len(), 1);
        assert_eq!(scan.coverage.identical_bytes, content.len() as u64);
        assert!(scan.coverage.identical[0]
            .paths
            .contains(&duplicate.canonicalize()?));
        assert_eq!(fs::read_to_string(active)?, content);
        Ok(())
    }

    #[test]
    fn equal_tails_need_full_verification_before_counting_bytes() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().canonicalize()?;
        let active = root.join("active.log");
        let mut data = b"event 23\n".repeat(WINDOW as usize / 8 + 100);
        fs::write(&active, &data)?;
        fs::write(root.join("copy.log"), &data)?;
        data[0] = b'X';
        fs::write(root.join("same-tail.log"), &data)?;
        let mut hits = Vec::new();
        let mut scan = Scan {
            deadline: Instant::now() + DURATION,
            samples: &[],
            matches: &mut hits,
            coverage: Coverage::default(),
        };
        scan_files(
            &mut scan,
            &HashSet::new(),
            std::slice::from_ref(&active),
            vec![root.clone()],
            &root.join("data"),
            &[],
        );
        assert_eq!(scan.coverage.identical_bytes, data.len() as u64);
        assert_eq!(
            scan.coverage.identical[0].paths,
            vec![active, root.join("copy.log")]
        );
        Ok(())
    }

    #[test]
    fn bounds_samples_and_excludes_partial_lines() -> Result<()> {
        use std::io::Write;
        let root = tempfile::tempdir()?;
        let file = root.path().join("events.log");
        let rows = "valid\n".repeat(210);
        fs::write(
            &file,
            format!("{}\n{rows}unfinished", "x".repeat(WINDOW as usize)),
        )?;
        let result = sample_file(&file)?;
        assert_eq!(result.sample.len(), ROWS);
        assert!(result.tail_hash.1 > WINDOW);
        assert!(result
            .sample
            .iter()
            .all(|line| Some(*line) == fingerprint("valid", None)));
        let gzip = root.path().join("events.log.gz");
        let mut encoder =
            flate2::write::GzEncoder::new(fs::File::create(&gzip)?, flate2::Compression::fast());
        encoder.write_all(rows.as_bytes())?;
        encoder.finish()?;
        assert_eq!(sample_file(&gzip)?.sample, result.sample);
        fs::write(&file, b"unfinished")?;
        assert!(sample_file(&file)?.sample.is_empty());
        Ok(())
    }
}
