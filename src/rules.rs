//! Classi condivise e limiti degli archivi. Le sorgenti esistenti restano indipendenti.
use crate::{
    archive::Archive,
    model::{Config, Retention, Source},
    proof, util,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub reason: String,
    pub period: Retention,
    #[serde(default)]
    pub max_mb: u32,
    #[serde(default)]
    pub seal: bool,
    #[serde(default)]
    pub tsa: Option<String>,
    #[serde(default)]
    pub mask_ips: bool,
    #[serde(default)]
    pub compress: bool,
    #[serde(default)]
    pub backup: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rules {
    pub classes: BTreeMap<String, Class>,
    pub max_mb: u32,
    pub remote: Option<crate::remote_backup::Schedule>,
}

impl Class {
    fn apply(&self, dir: &Path, source: &mut Source) -> Result<()> {
        ensure!(
            !self.mask_ips
                || source.privacy.mask_ips_after_days == Some(0)
                || !proof::chain_path(dir, &source.id).is_file(),
            "La classe modificherebbe il testo già sigillato della sorgente {}",
            source.id
        );
        source.retention = Some(self.period);
        if self.period.unit == crate::model::RetentionUnit::Days {
            source.retention_days = self.period.amount;
        }
        source.proof = self.seal.then(|| proof::Proof {
            tsa: self.tsa.clone(),
        });
        source.privacy.mask_ips_after_days = self.mask_ips.then_some(0);
        Ok(())
    }
}

impl Rules {
    pub fn validate(&self, sources: &[Source]) -> Result<()> {
        for (name, class) in &self.classes {
            ensure!(
                !name.trim().is_empty() && name.len() <= 120 && !name.chars().any(char::is_control),
                "Nome della classe non valido"
            );
            ensure!(
                !class.reason.trim().is_empty(),
                "Indica il motivo della classe {name}"
            );
            class.period.validate()?;
            ensure!(
                class
                    .tsa
                    .as_ref()
                    .is_none_or(|url| class.seal && url.starts_with("https://")),
                "La marca richiede sigillo e un indirizzo HTTPS"
            );
        }
        for s in sources {
            ensure!(
                s.site.len() <= 200 && !s.site.chars().any(char::is_control),
                "Nome del sito non valido"
            );
            if let Some(name) = &s.class {
                let class = self.classes.get(name).context("Classe inesistente")?;
                ensure!(
                    s.period() == class.period
                        && s.proof.as_ref().map(|p| &p.tsa) == class.seal.then_some(&class.tsa)
                        && s.privacy.mask_ips_after_days == class.mask_ips.then_some(0),
                    "La sorgente {} segue la classe {name}: modifica la classe o scollegala prima",
                    s.id
                );
            }
        }
        if let Some(plan) = &self.remote {
            plan.validate()?;
        }
        Ok(())
    }
}

pub fn maintain(dir: &Path, cfg: &Config) -> Result<()> {
    let mut errors = Vec::new();
    for (name, class) in &cfg.rules.classes {
        let sources: Vec<_> = cfg
            .sources
            .iter()
            .filter(|s| s.class.as_ref() == Some(name))
            .collect();
        if class.compress {
            for s in &sources {
                let path = s.archive_path(dir);
                if path.exists() {
                    if let Err(e) = Archive::open(&path).and_then(|db| db.compress_closed()) {
                        errors.push(format!("{}: {e:#}", s.id));
                    }
                }
            }
        }
        if let Err(e) = limit(dir, &sources, class.max_mb) {
            errors.push(format!("{name}: {e:#}"));
        }
    }
    if cfg.rules.max_mb > 0 {
        if let Err(e) = limit(
            dir,
            &cfg.sources.iter().collect::<Vec<_>>(),
            cfg.rules.max_mb,
        ) {
            errors.push(format!("Quota totale: {e:#}"));
        }
    }
    ensure!(errors.is_empty(), "{}", errors.join("\n"));
    Ok(())
}

fn limit(dir: &Path, sources: &[&Source], mb: u32) -> Result<()> {
    if mb == 0 {
        return Ok(());
    }
    let ceiling = u64::from(mb) * 1024 * 1024;
    let bytes = || {
        sources
            .iter()
            .map(|s| util::bytes_on_disk(&s.archive_path(dir)))
            .sum::<u64>()
    };
    if bytes() <= ceiling {
        return Ok(());
    }
    let mut dbs = Vec::new();
    for s in sources {
        let path = s.archive_path(dir);
        if path.exists() {
            let db = Archive::open(&path)?;
            compact(&db)?;
            dbs.push((s, db));
        }
    }
    // Usare le pagine occupate evita cancellazioni superflue quando un lettore trattiene il WAL.
    loop {
        let mut used = 0u64;
        let mut oldest = Vec::new();
        for (idx, (s, db)) in dbs.iter().enumerate() {
            used+=db.db.query_row("SELECT (page_count-freelist_count)*page_size FROM pragma_page_count,pragma_freelist_count,pragma_page_size",[],|r|r.get::<_,i64>(0))? as u64;
            if s.proof.is_none() && !proof::chain_path(dir, &s.id).exists() {
                if let Some(time) =
                    db.db
                        .query_row("SELECT min(event_ns) FROM entries", [], |r| {
                            r.get::<_, Option<i64>>(0)
                        })?
                {
                    oldest.push((time, idx));
                }
            }
        }
        if used <= ceiling {
            break;
        }
        oldest.sort_unstable();
        let (_, idx) = *oldest
            .first()
            .context("Quota insufficiente: restano archivi sigillati o metadati SQLite")?;
        let cutoff = oldest.get(1).map_or(i64::MAX, |v| v.0);
        let db = &dbs[idx].1.db;
        // Stima il lotto dai byte effettivi delle righe per non svuotare un archivio
        // quando bastano pochi messaggi grandi.
        let mut sizes=db.prepare("SELECT length(CAST(message AS blob)) FROM entries WHERE event_ns<=? ORDER BY event_ns,id LIMIT 256")?;
        let mut freed = 0u64;
        let mut batch = 0i64;
        for size in sizes.query_map([cutoff], |row| row.get::<_, i64>(0))? {
            batch += 1;
            freed += size? as u64;
            if freed >= used - ceiling {
                break;
            }
        }
        db.execute("DELETE FROM entries WHERE id IN (SELECT id FROM entries WHERE event_ns<=?1 ORDER BY event_ns,id LIMIT ?2)",rusqlite::params![cutoff,batch])?;
    }
    for (_, db) in dbs {
        compact(&db)?;
    }
    ensure!(
        bytes() <= ceiling,
        "Quota in attesa del rilascio delle pagine SQLite"
    );
    Ok(())
}

fn compact(store: &Archive) -> Result<()> {
    // incremental_vacuum restituisce una riga per pagina: SQLite deve arrivare a DONE.
    let db = &store.db;
    let auto: i64 = db.pragma_query_value(None, "auto_vacuum", |r| r.get(0))?;
    if auto == 0 {
        if db.pragma_query_value(None, "freelist_count", |r| r.get::<_, i64>(0))? > 0 {
            db.execute_batch("VACUUM")?;
        }
    } else {
        let mut stmt = db.prepare("PRAGMA incremental_vacuum")?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
    }
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Change {
    Class {
        name: String,
        class: Class,
    },
    RemoveClass {
        name: String,
    },
    Assign {
        source: String,
        class: Option<String>,
        site: String,
    },
    Space {
        max_mb: u32,
    },
    Proof {
        source: String,
        enabled: bool,
        tsa: Option<String>,
    },
    Alert {
        rule: crate::alerts::Rule,
    },
    RemoveAlert {
        name: String,
    },
    Schedule {
        schedule: Option<crate::remote_backup::Schedule>,
    },
}
pub fn change(dir: &Path, body: serde_json::Value) -> Result<()> {
    let action: Change = serde_json::from_value(body)?;
    crate::config::edit(dir, |cfg| {
        match action {
            Change::Class { name, class } => {
                for s in cfg
                    .sources
                    .iter_mut()
                    .filter(|s| s.class.as_ref() == Some(&name))
                {
                    class.apply(dir, s)?;
                }
                cfg.rules.classes.insert(name, class);
            }
            Change::RemoveClass { name } => {
                ensure!(
                    !cfg.sources.iter().any(|s| s.class.as_ref() == Some(&name)),
                    "Riassegna prima le sorgenti della classe"
                );
                cfg.rules
                    .classes
                    .remove(&name)
                    .context("Classe inesistente")?;
            }
            Change::Assign {
                source,
                class,
                site,
            } => {
                let s = cfg
                    .sources
                    .iter_mut()
                    .find(|s| s.id == source)
                    .context("Sorgente inesistente")?;
                if let Some(name) = &class {
                    cfg.rules
                        .classes
                        .get(name)
                        .context("Classe inesistente")?
                        .apply(dir, s)?;
                }
                s.class = class;
                s.site = site.trim().into();
            }
            Change::Space { max_mb } => cfg.rules.max_mb = max_mb,
            Change::Proof {
                source,
                enabled,
                tsa,
            } => {
                let s = cfg
                    .sources
                    .iter_mut()
                    .find(|s| s.id == source)
                    .context("Sorgente inesistente")?;
                ensure!(
                    tsa.as_deref().is_none_or(|v| v.starts_with("https://")),
                    "Indica una TSA HTTPS"
                );
                s.proof = enabled.then_some(proof::Proof { tsa });
            }
            Change::Alert { rule } => {
                if let crate::alerts::Check::FreeSpace { megabytes }
                | crate::alerts::Check::ArchiveSize { megabytes, .. } = &rule.check
                {
                    ensure!(
                        *megabytes > 0 && *megabytes <= u32::MAX as u64,
                        "Soglia in MB non valida"
                    );
                }
                cfg.alerts.rules.retain(|r| r.name != rule.name);
                cfg.alerts.rules.push(rule);
            }
            Change::RemoveAlert { name } => cfg.alerts.rules.retain(|r| r.name != name),
            Change::Schedule { schedule } => cfg.rules.remote = schedule,
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Entry, query::Query, timestamps};
    use tempfile::tempdir;

    fn entries(store: &mut Archive, source: &str, at: i64, count: usize) {
        let rows = (0..count)
            .map(|n| Entry {
                key: format!("{source}-{n}"),
                file_id: None,
                generation: 0,
                offset: 0,
                event_ns: at + n as i64,
                collected_ns: at + n as i64,
                timestamp_origin: "event",
                message: format!("marker-{source}-{n} {}", "repeat ".repeat(650)),
            })
            .collect::<Vec<_>>();
        store.save_entries(&rows).unwrap();
    }

    #[test]
    fn compression_preserves_search_proof_and_verified_backup() {
        let dir = tempdir().unwrap();
        let s = Source {
            id: "sealed".into(),
            proof: Some(proof::Proof::default()),
            input: crate::model::Input::Folder {
                path: dir.path().to_owned(),
            },
            ..Default::default()
        };
        crate::config::add(dir.path(), s.clone()).unwrap();
        let now = timestamps::now_ns();
        let mut store = Archive::open(&s.archive_path(dir.path())).unwrap();
        entries(&mut store, "old", now - 2 * 86_400_000_000_000, 3);
        entries(&mut store, "today", now, 1);
        proof::seal(dir.path(), &s, &store, now).unwrap();
        store.compress_closed().unwrap();
        store.compress_closed().unwrap();
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT count(*) FROM entries WHERE typeof(message)='blob'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        assert_eq!(store.count_since(0, "marker-old").unwrap(), 3);
        assert_eq!(
            store
                .search("sealed", &Query::text("marker-old"), 0, i64::MAX, 10)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            proof::verify_source(dir.path(), "sealed").unwrap().problems,
            0
        );
        let dest = tempdir().unwrap();
        crate::backup::create(dir.path(), dest.path()).unwrap();
        crate::backup::verify(dest.path()).unwrap();
        let restored = tempdir().unwrap();
        crate::backup::restore(restored.path(), dest.path(), false).unwrap();
        assert_eq!(
            proof::verify_source(restored.path(), "sealed")
                .unwrap()
                .problems,
            0
        );
    }

    #[test]
    fn shared_quota_reclaims_oldest_across_archives_and_preserves_seals() {
        let dir = tempdir().unwrap();
        let old = Source {
            id: "old".into(),
            ..Default::default()
        };
        let new = Source {
            id: "new".into(),
            ..Default::default()
        };
        let sealed = Source {
            id: "proof".into(),
            proof: Some(proof::Proof::default()),
            ..Default::default()
        };
        let sources = [&old, &new, &sealed];
        for (n, s) in sources.iter().enumerate() {
            let mut db = Archive::open(&s.archive_path(dir.path())).unwrap();
            entries(&mut db, &s.id, 1_000_000 * n as i64, 200);
        }
        limit(dir.path(), &sources, 2).unwrap();
        let bytes: u64 = sources
            .iter()
            .map(|s| util::bytes_on_disk(&s.archive_path(dir.path())))
            .sum();
        assert!(bytes <= 2 * 1024 * 1024, "{bytes}");
        let count = |s: &Source| {
            Archive::open(&s.archive_path(dir.path()))
                .unwrap()
                .stats()
                .unwrap()
                .0
        };
        assert!(count(&old) < 200);
        assert_eq!(count(&sealed), 200);
        assert!(count(&new) == 200 || count(&old) == 0);
    }
}
