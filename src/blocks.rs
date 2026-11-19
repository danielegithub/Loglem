//! The SQLite tail and its packed blocks share one monotonically increasing row id.
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::io::Write;

const ROWS: usize = 1000;
const BYTES: usize = 2 * 1024 * 1024;
pub(crate) const VIEW: &str = "
    CREATE TEMP VIEW log_rows AS
      SELECT *,NULL AS block_id,id AS last_id,event_ns AS last_ns,event_ns AS first_ns FROM hot_rows
      UNION ALL
      SELECT j.value->>0,j.value->>1,f.name,b.generation,j.value->>2,j.value->>3,j.value->>4,
             j.value->>5,j.value->>6,b.id,b.last_id,b.last_ns,b.first_ns
      FROM blocks b LEFT JOIN files f ON f.id=b.file_ref
      CROSS JOIN json_each(loglem_decode(b.data)) j";
// id, deduplication key, byte offset, event time, collection time, timestamp origin, message
type Row = (i64, String, i64, i64, i64, String, String);

/// Toglie le chiavi di un blocco: si ricavano dal blocco stesso, quindi prima di cancellarlo.
fn drop_keys(db: &Connection, block: i64) -> Result<()> {
    db.execute(
        "DELETE FROM block_keys WHERE k IN (SELECT loglem_key(j.value->>1) FROM blocks b \
         CROSS JOIN json_each(loglem_decode(b.data)) j WHERE b.id=?)",
        [block],
    )?;
    Ok(())
}

pub(crate) fn pack(db: &Connection, now: i64) -> Result<()> {
    let cutoff = now.saturating_sub(300_000_000_000);
    let mut packed = 0;
    loop {
        let tx = db.unchecked_transaction()?;
        let group = tx.query_row(
            "SELECT file_id,generation FROM (
               SELECT file_id,generation FROM entries WHERE collected_ns<=?1 LIMIT 1
             ) UNION ALL SELECT file_id,generation FROM (
               SELECT file_id,generation FROM entries GROUP BY file_id,generation HAVING count(*)>=?2
             ) LIMIT 1",
            params![cutoff, ROWS as i64], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        ).optional()?;
        let Some((file, generation)) = group else {
            drop(tx);
            // A large migration may release many more pages than ordinary retention.
            if packed > 1
                && db.pragma_query_value(None, "freelist_count", |r| r.get::<_, i64>(0))? >= 256
            {
                let mut vacuum = db.prepare("PRAGMA incremental_vacuum")?;
                let mut pages = vacuum.query([])?;
                while pages.next()?.is_some() {}
            }
            return Ok(());
        };
        let mut stmt = tx.prepare(
            "SELECT id,record_key,byte_offset,event_ns,collected_ns,timestamp_origin,message
             FROM hot_rows WHERE file_id IS ?1 AND generation=?2 ORDER BY id LIMIT ?3",
        )?;
        let mut rows = stmt.query(params![file, generation, ROWS as i64])?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"[\n")?;
        let mut ids = Vec::with_capacity(ROWS);
        let mut keys = Vec::new();
        let mut low = [i64::MAX; 3];
        let mut high = [i64::MIN; 3];
        let mut bytes = 0;
        while let Some(r) = rows.next()? {
            let row: Row = (
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            );
            let encoded = serde_json::to_vec(&row)?;
            if !ids.is_empty() && bytes + encoded.len() > BYTES {
                break;
            }
            if !ids.is_empty() {
                encoder.write_all(b",\n")?;
            }
            encoder.write_all(&encoded)?;
            ids.push(row.0);
            for (i, v) in [row.2, row.3, row.4].into_iter().enumerate() {
                low[i] = low[i].min(v);
                high[i] = high[i].max(v);
            }
            bytes += encoded.len() + 2;
            if file
                .as_ref()
                .is_none_or(|f| row.1 != format!("file:{f}:{generation}:{}", row.2))
            {
                keys.push(row.1);
            }
            if bytes >= BYTES {
                break;
            }
        }
        drop(rows);
        drop(stmt);
        ensure!(!ids.is_empty(), "Blocco vuoto");
        encoder.write_all(b"\n]")?;
        let data = encoder.finish()?;
        if let Some(f) = &file {
            tx.execute("INSERT OR IGNORE INTO files(name) VALUES(?)", [f])?;
        }
        tx.execute("INSERT INTO blocks(file_ref,generation,first_id,last_id,first_offset,last_offset,first_ns,last_ns,first_col,last_col,lines,data) VALUES((SELECT id FROM files WHERE name=?),?,?,?,?,?,?,?,?,?,?,?)",
            params![file,generation,ids[0],ids.last(),low[0],high[0],low[1],high[1],low[2],high[2],ids.len() as i64,data])?;
        let block = tx.last_insert_rowid();
        for key in keys {
            tx.execute("INSERT INTO block_keys VALUES(loglem_key(?),?)", params![key, block])?;
        }
        tx.execute_batch("INSERT INTO state VALUES('moving_block','1')")?;
        tx.execute(
            "DELETE FROM entries WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&ids)?],
        )?;
        tx.execute_batch("DELETE FROM state WHERE key='moving_block'")?;
        tx.commit()?;
        packed += 1;
    }
}

/// Materialize only selected blocks before an operation that changes their rows.
/// Moving the rows and dropping the old blob are one transaction.
pub(crate) fn unpack(
    db: &Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<bool> {
    let tx = db.unchecked_transaction()?;
    let mut stmt = tx.prepare(&format!("SELECT id FROM blocks WHERE {predicate}"))?;
    let ids = stmt
        .query_map(values, |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if ids.is_empty() {
        return Ok(false);
    }
    tx.execute_batch("INSERT INTO state VALUES('moving_block','1')")?;
    for id in ids {
        tx.execute("INSERT INTO entries SELECT id,record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,message FROM log_rows WHERE block_id=?", [id])?;
        drop_keys(&tx, id)?;
        tx.execute("DELETE FROM blocks WHERE id=?", [id])?;
    }
    tx.execute_batch("DELETE FROM state WHERE key='moving_block'")?;
    tx.commit()?;
    Ok(true)
}

pub(crate) fn search(
    db: &Connection,
    source: &str,
    query: &crate::query::Query,
    from: i64,
    until: i64,
    limit: usize,
) -> Result<Vec<crate::model::SearchRow>> {
    use rusqlite::types::Value;
    use std::{
        collections::BTreeMap,
        io::{BufRead, BufReader},
    };
    let mut best = BTreeMap::new();
    let limit = limit.min(1000);
    if limit == 0 {
        return Ok(vec![]);
    }
    let make = |at, origin: String, message: String| crate::model::SearchRow {
        source: source.into(),
        timestamp: crate::timestamps::format_ns(at),
        timestamp_origin: origin,
        level: crate::query::severity(&message).name().into(),
        message,
    };
    let (filters, mut args) = query.sql();
    args.insert(0, Value::Integer(from));
    args.insert(1, Value::Integer(until));
    args.push(Value::Integer(limit as i64));
    let tx = db.unchecked_transaction()?;
    let mut hot=tx.prepare(&format!("SELECT event_ns,id,timestamp_origin,message FROM hot_rows WHERE event_ns>=? AND event_ns<?{filters} ORDER BY event_ns DESC,id DESC LIMIT ?"))?;
    let mut rows = hot.query(rusqlite::params_from_iter(args))?;
    while let Some(row) = rows.next()? {
        let at = row.get(0)?;
        best.insert(
            (at, row.get::<_, i64>(1)?),
            make(at, row.get(2)?, row.get(3)?),
        );
    }
    let escaped = serde_json::to_string(&query.text)?;
    let needle = memchr::memmem::Finder::new(&escaped.as_bytes()[1..escaped.len() - 1]);
    let mut stmt = tx.prepare(
        "SELECT last_ns,data FROM blocks WHERE last_ns>=? AND first_ns<? ORDER BY last_ns DESC",
    )?;
    let mut blocks = stmt.query(params![from, until])?;
    let mut line = Vec::new();
    while let Some(block) = blocks.next()? {
        if best.len() == limit && block.get::<_, i64>(0)? < best.first_key_value().unwrap().0 .0 {
            break;
        }
        let mut reader = BufReader::new(flate2::read::GzDecoder::new(block.get_ref(1)?.as_blob()?));
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            let mut data = line.strip_suffix(b"\n").unwrap_or(&line);
            data = data.strip_suffix(b",").unwrap_or(data);
            if data == b"[" || data == b"]" {
                continue;
            }
            if !query.ignore_case && !query.text.is_empty() && needle.find(data).is_none() {
                continue;
            }
            let row: Row = serde_json::from_slice(data)?;
            if row.3 >= from && row.3 < until && query.matches(&row.6) {
                let key = (row.3, row.0);
                if best.len() == limit && key <= *best.first_key_value().unwrap().0 {
                    continue;
                }
                best.insert(key, make(row.3, row.5, row.6));
                if best.len() > limit {
                    best.pop_first();
                }
            }
        }
    }
    Ok(best.into_values().rev().collect())
}

/// Whole expired blocks can be dropped without inflating their records into the tail.
pub(crate) fn discard(
    db: &Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<usize> {
    let tx = db.unchecked_transaction()?;
    let mut stmt = tx.prepare(&format!(
        "SELECT id,lines,first_ns,last_ns FROM blocks WHERE {predicate}"
    ))?;
    let ids = stmt
        .query_map(values, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut removed = 0;
    for (id, n, first, last) in ids {
        let hits: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM alert_hits WHERE at BETWEEN ? AND ?)",
            params![first, last],
            |r| r.get(0),
        )?;
        if hits {
            tx.execute(
                "DELETE FROM alert_hits WHERE id IN (SELECT id FROM log_rows WHERE block_id=?)",
                [id],
            )?;
        }
        tx.execute(
            "UPDATE daily_stats SET upto=0,summary=NULL,dirty=1 WHERE day BETWEEN ? AND ?",
            params![
                first.div_euclid(86_400_000_000_000),
                last.div_euclid(86_400_000_000_000)
            ],
        )?;
        drop_keys(&tx, id)?;
        tx.execute("DELETE FROM blocks WHERE id=?", [id])?;
        removed += n;
    }
    if removed > 0 {
        tx.execute("UPDATE entry_total SET n=n-?, first_ns=(SELECT min(t) FROM (SELECT min(event_ns) t FROM entries UNION ALL SELECT min(first_ns) FROM blocks))", [removed])?;
    }
    tx.commit()?;
    Ok(removed as usize)
}

#[cfg(test)]
mod tests {
    use crate::{
        archive::Archive,
        backup, config, erase,
        model::{Entry, Source},
        privacy,
        query::Query,
        stats, timestamps,
    };
    use anyhow::Result;

    #[test]
    fn packed_rows_survive_replay_restore_redaction_and_partial_expiry() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let dir = tmp.path().join("live");
        let source = Source {
            id: "site".into(),
            input: crate::model::Input::Folder {
                path: tmp.path().into(),
                pattern: None,
            },
            ..Default::default()
        };
        config::add(&dir, source.clone())?;
        let now = timestamps::now_ns();
        let rows: Vec<_> = (0..1010)
            .map(|n| Entry {
                key: format!("file:unit:0:{n}"),
                file_id: Some("unit".into()),
                generation: 0,
                offset: n,
                event_ns: now - 1010 + n,
                collected_ns: now,
                timestamp_origin: "event",
                message: format!(
                    "192.0.2.1 - - [time] \"GET /{} HTTP/1.1\" 200 20",
                    if n == 3 { "forget" } else { "keep" }
                ),
            })
            .collect();
        let mut db = Archive::open(&source.archive_path(&dir))?;
        assert_eq!(db.save_entries(&rows)?, 1010);
        assert_eq!(db.save_entries(&rows)?, 0);
        assert_eq!(db.stats()?.0, 1010);
        stats::refresh(&db)?;
        let before = stats::compute(&dir, "site", 1)?;
        assert_eq!(before.requests, 1010);
        assert_eq!(before.top_ips, vec![("192.0.2.1".into(), 1010)]);
        assert_eq!(db.pattern_hits(now - 2000, "forget", 1)?, 1);
        drop(db);
        let saved = tmp.path().join("snapshot");
        backup::create(&dir, &saved)?;
        let restored = tmp.path().join("restored");
        crate::util::private_dir(&restored)?;
        backup::restore(&restored, &saved, false)?;
        let read = Archive::open(&source.archive_path(&restored))?;
        assert_eq!(
            read.search("site", &Query::text("forget"), 0, i64::MAX, 10)?
                .len(),
            1
        );
        erase::erase(
            &dir,
            Some("site"),
            &Query::text("forget"),
            "test",
            "test",
            false,
        )?;
        let mut db = Archive::open(&source.archive_path(&dir))?;
        assert_eq!(db.pattern_hits(now - 2000, "forget", 1)?, 0);
        assert_eq!(stats::compute(&dir, "site", 1)?.requests, 1009);
        privacy::apply(
            &db,
            &privacy::Privacy {
                mask_ips_after_days: Some(0),
                ..Default::default()
            },
            now,
        )?;
        db.compress_closed()?;
        assert!(stats::compute(&dir, "site", 1)?
            .top_ips
            .iter()
            .all(|(ip, _)| ip != "192.0.2.1"));
        assert_eq!(db.retain_since(now - 500)?, 509);
        assert_eq!(db.stats()?, (500, Some(now - 500)));
        assert_eq!(stats::compute(&dir, "site", 1)?.requests, 500);
        assert_eq!(db.retain_since(now + 1)?, 500);
        assert_eq!(db.stats()?.0, 0);
        Ok(())
    }
}

#[cfg(test)]
mod legacy_proof {
    use crate::{archive::Archive, config, model::Source, proof};
    use anyhow::Result;
    use std::io::Write;

    #[test]
    fn a_seal_over_legacy_rows_verifies_after_migration_and_packing() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let source = Source {
            id: "auth".into(),
            proof: Some(proof::Proof { tsa: None }),
            ..Default::default()
        };
        let path = config::archive_path(tmp.path(), &source.id);
        crate::util::private_dir(path.parent().unwrap())?;
        let legacy = rusqlite::Connection::open(&path)?;
        legacy.execute_batch(include_str!("schema.sql"))?;
        let day = 86_400_000_000_000i64;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"archived event")?;
        legacy.execute(
            "INSERT INTO entries VALUES(7,'journal:alpha',NULL,0,0,?1,?1,'event',?2)",
            rusqlite::params![day, encoder.finish()?],
        )?;
        legacy.execute_batch("PRAGMA user_version=3")?;
        drop(legacy);
        // Fixture from the v3 seal format, computed independently from the verifier.
        let chain = proof::chain_path(tmp.path(), "auth");
        crate::util::private_dir(chain.parent().unwrap())?;
        std::fs::write(
            chain,
            r#"{"event":"sealed","day":"1970-01-02","count":1,"hash":"ad385880491a880d93f70924c362ea58c676746690ad3a7e10977cdb549a824d","previous":"inizio","sealed_at":"1970-01-03T00:00:00Z"}"#,
        )?;
        let migrated = Archive::open(&path)?;
        assert_eq!(proof::verify_source(tmp.path(), "auth")?.problems, 0);
        migrated.compress_closed()?;
        assert_eq!(
            migrated
                .db
                .query_row("SELECT count(*) FROM entries", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(proof::verify_source(tmp.path(), "auth")?.problems, 0);
        Ok(())
    }
}
