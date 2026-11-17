//! Dati di una persona: cancellazione (GDPR art. 17) ed esportazione (art. 15).
//!
//! Si sceglie con gli stessi filtri della ricerca (IP, testo, percorso, codice). La
//! cancellazione mostra prima quante righe tocca. Agisce solo sugli archivi di Loglem:
//! i file originali, i log di Podman e del journal e i backup restano com'erano.
use crate::{
    archive::Archive,
    audit, config,
    model::Source,
    originals::{self, FileOutcome},
    query::Query,
    util,
};
use anyhow::{bail, Result};
use rusqlite::params_from_iter;
use std::{collections::BTreeMap, io::Write, path::Path};

/// Per cancellare o esportare serve almeno un filtro che identifichi la persona: la sola
/// gravità non basta (cancellerebbe tutti gli errori).
fn require_filter(query: &Query) -> Result<()> {
    let has = |value: &Option<String>| value.as_deref().is_some_and(|s| !s.is_empty());
    if query.text.is_empty() && !has(&query.ip) && !has(&query.path) && query.status.is_none() {
        bail!("Indica chi cercare: --ip, --text, --path o --status");
    }
    Ok(())
}

fn selected<'a>(sources: &'a [Source], wanted: Option<&str>) -> Result<Vec<&'a Source>> {
    let found: Vec<&Source> = sources
        .iter()
        .filter(|s| wanted.is_none_or(|w| w == s.id))
        .collect();
    if found.is_empty() {
        bail!("Sorgente non trovata");
    }
    Ok(found)
}

/// Quante righe corrispondono, per sorgente.
pub fn preview(dir: &Path, source: Option<&str>, query: &Query) -> Result<Vec<(String, i64)>> {
    require_filter(query)?;
    let _access = util::DataAccess::read(dir)?;
    let (filters, values) = query.sql();
    let mut counts = Vec::new();
    for s in selected(&config::load(dir)?.sources, source)? {
        let path = config::archive_path(dir, &s.id);
        if !path.exists() {
            continue;
        }
        let archive = Archive::open(&path)?;
        let count: i64 = archive.db.query_row(
            &format!("SELECT count(*) FROM entries WHERE 1 = 1{filters}"),
            params_from_iter(values.clone()),
            |r| r.get(0),
        )?;
        counts.push((s.id.clone(), count));
    }
    Ok(counts)
}

pub struct Erased {
    pub rows: i64,
    pub sources: usize,
    /// Giorni già sigillati la cui impronta è stata ricalcolata.
    pub resealed_days: usize,
    /// Cosa è successo ai file originali, per sorgente (solo con `originals`).
    pub originals: Vec<(String, Vec<FileOutcome>)>,
}

/// Cosa succederebbe ai file originali: per ogni sorgente, i file con le righe da togliere
/// e quelli che verrebbero saltati (e perché). Non modifica nulla.
pub fn preview_originals(
    dir: &Path,
    source: Option<&str>,
    query: &Query,
) -> Result<Vec<(String, Vec<FileOutcome>)>> {
    require_filter(query)?;
    let _access = util::DataAccess::read(dir)?;
    let mut all = Vec::new();
    for s in selected(&config::load(dir)?.sources, source)? {
        let path = config::archive_path(dir, &s.id);
        if !path.exists() {
            continue;
        }
        let mut archive = Archive::open(&path)?;
        all.push((s.id.clone(), originals::run(s, &mut archive, query, false)?));
    }
    Ok(all)
}

/// Cancella le righe corrispondenti da tutte le sorgenti (o da una) e lo scrive nel registro
/// degli accessi e, per le sorgenti in modalità prova, nel registro della prova.
pub fn erase(
    dir: &Path,
    source: Option<&str>,
    query: &Query,
    reason: &str,
    user: &str,
    include_originals: bool,
) -> Result<Erased> {
    require_filter(query)?;
    if reason.trim().is_empty() {
        bail!("Indica il motivo con --reason (es. «richiesta art. 17 del 17/11/2026, pratica 3»)");
    }
    let _access = util::DataAccess::write(dir)?;
    let now = crate::timestamps::now_ns();
    let (filters, values) = query.sql();
    let mut result = Erased {
        rows: 0,
        sources: 0,
        resealed_days: 0,
        originals: Vec::new(),
    };
    for s in selected(&config::load(dir)?.sources, source)? {
        let path = config::archive_path(dir, &s.id);
        if !path.exists() {
            continue;
        }
        let mut archive = Archive::open(&path)?;
        // Righe per giorno di raccolta: la modalità prova deve sapere quali giorni cambiano.
        let mut by_day = BTreeMap::new();
        {
            let mut select = archive.db.prepare(&format!(
                "SELECT strftime('%Y-%m-%d', collected_ns / 1000000000, 'unixepoch'), count(*) \
                 FROM entries WHERE 1 = 1{filters} GROUP BY 1"
            ))?;
            let mut rows = select.query(params_from_iter(values.clone()))?;
            while let Some(row) = rows.next()? {
                by_day.insert(row.get::<_, String>(0)?, row.get::<_, i64>(1)?);
            }
        }
        let removed: i64 = by_day.values().sum();
        if include_originals {
            // Prima i file originali: se qualcosa va storto lì, l'archivio non è ancora toccato.
            result
                .originals
                .push((s.id.clone(), originals::run(s, &mut archive, query, true)?));
        }
        if removed == 0 {
            continue;
        }
        archive.db.execute(
            &format!(
                "DELETE FROM entries WHERE id IN (SELECT id FROM entries WHERE 1 = 1{filters})"
            ),
            params_from_iter(values.clone()),
        )?;
        // Il contenuto cancellato non deve restare nei file di appoggio di SQLite.
        archive
            .db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA incremental_vacuum;")?;
        if s.proof.is_some() {
            let note = format!("cancellazione su richiesta: {reason}");
            result.resealed_days += crate::proof::amend(dir, s, &archive, &by_day, &note, now)?;
        }
        result.rows += removed;
        result.sources += 1;
    }
    audit::record(
        dir,
        user,
        "cancellazione",
        &format!(
            "{} righe in {} sorgenti; file originali riscritti: {}; filtri: {}; motivo: {}",
            result.rows,
            result.sources,
            result
                .originals
                .iter()
                .flat_map(|(_, files)| files)
                .filter(|f| f.status == "rewritten")
                .count(),
            audit::filter_kinds(query),
            reason.trim()
        ),
    );
    Ok(result)
}

/// Scrive le righe corrispondenti, dalla più vecchia: testo semplice o una riga JSON per riga.
pub fn export(
    dir: &Path,
    source: Option<&str>,
    query: &Query,
    out: &mut dyn Write,
    json: bool,
    user: &str,
) -> Result<u64> {
    require_filter(query)?;
    let _access = util::DataAccess::read(dir)?;
    let (filters, values) = query.sql();
    let mut written = 0;
    for s in selected(&config::load(dir)?.sources, source)? {
        let path = config::archive_path(dir, &s.id);
        if !path.exists() {
            continue;
        }
        let archive = Archive::open(&path)?;
        let mut select = archive.db.prepare(&format!(
            "SELECT event_ns, timestamp_origin, message FROM entries WHERE 1 = 1{filters} \
             ORDER BY event_ns, id"
        ))?;
        let mut rows = select.query(params_from_iter(values.clone()))?;
        while let Some(row) = rows.next()? {
            let time = crate::timestamps::format_ns(row.get(0)?);
            let (origin, message): (String, String) = (row.get(1)?, row.get(2)?);
            if json {
                let line = serde_json::json!({
                    "source": s.id, "time": time, "time_origin": origin, "message": message
                });
                writeln!(out, "{line}")?;
            } else {
                writeln!(out, "{time} [{}] {message}", s.id)?;
            }
            written += 1;
        }
    }
    audit::record(
        dir,
        user,
        "esportazione",
        &format!("{written} righe; filtri: {}", audit::filter_kinds(query)),
    );
    Ok(written)
}
