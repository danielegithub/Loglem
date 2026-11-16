use crate::{
    model::{Cursor, Entry, SearchRow},
    timestamps::format_ns,
    util::private_dir,
};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

pub struct Archive {
    pub(crate) db: Connection,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        private_dir(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Missing archive directory"))?,
        )?;
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            bail!("Archive schema {version} needs a newer Loglem; no changes made");
        }
        if version == 0 {
            db.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {} COMMIT;",
                include_str!("schema.sql")
            ))?;
        }
        db.execute_batch(concat!(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; ",
            "PRAGMA cache_size=-512; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; ",
            "PRAGMA journal_size_limit=4194304;"
        ))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self { db })
    }

    pub fn cursor(&self, file_id: &str) -> Result<Cursor> {
        Ok(self
            .db
            .query_row(
                "SELECT byte_offset,generation,anchor FROM cursors WHERE file_id=?1",
                [file_id],
                |row| {
                    Ok(Cursor {
                        offset: row.get::<_, i64>(0)? as u64,
                        generation: row.get(1)?,
                        anchor: row.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    fn insert(tx: &Transaction<'_>, entries: &[Entry]) -> Result<usize> {
        let mut added = 0;
        let mut statement = tx.prepare_cached(concat!("INSERT OR IGNORE INTO ", "entries(record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,raw,message) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"))?;
        for entry in entries {
            added += statement.execute(params![
                entry.key,
                entry.file_id,
                entry.generation,
                entry.offset,
                entry.event_ns,
                entry.collected_ns,
                entry.timestamp_origin,
                entry.raw,
                entry.message
            ])?;
        }
        Ok(added)
    }

    /// Data and read position are committed together. A failed transaction advances neither.
    pub fn save_file_batch(
        &mut self,
        file_id: &str,
        cursor: &Cursor,
        entries: &[Entry],
    ) -> Result<usize> {
        let tx = self.db.transaction()?;
        let added = Self::insert(&tx, entries)?;
        tx.execute(concat!("INSERT INTO cursors VALUES(?1,?2,?3,?4) ON CONFLICT(file_id) DO UPDATE SET ", "byte_offset=excluded.byte_offset,generation=excluded.generation,anchor=excluded.anchor"), params![file_id, cursor.offset as i64, cursor.generation, cursor.anchor])?;
        tx.commit()?;
        Ok(added)
    }

    pub fn save_entries(&mut self, entries: &[Entry]) -> Result<usize> {
        let tx = self.db.transaction()?;
        let added = Self::insert(&tx, entries)?;
        tx.commit()?;
        Ok(added)
    }

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT value FROM state WHERE key=?1", [key], |r| r.get(0))
            .optional()?)
    }
    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO state VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn begin_podman_snapshot(&self) -> Result<()> {
        self.db.execute_batch(concat!(
            "DROP TABLE IF EXISTS temp.snapshot_counts; CREATE TEMP TABLE ",
            "snapshot_counts(key TEXT PRIMARY KEY, n INTEGER NOT NULL);"
        ))?;
        Ok(())
    }
    pub fn occurrence(&self, key: &str) -> Result<i64> {
        self.db.execute(
            "INSERT INTO snapshot_counts VALUES(?1,1) ON CONFLICT(key) DO UPDATE SET n=n+1",
            [key],
        )?;
        Ok(self
            .db
            .query_row("SELECT n FROM snapshot_counts WHERE key=?1", [key], |r| {
                r.get(0)
            })?)
    }

    pub fn retain_since(&mut self, cutoff: i64) -> Result<usize> {
        // Delete in bounded transactions so retention cannot monopolize the archive.
        let mut deleted = 0;
        loop {
            let n = self.db.execute(
                concat!(
                    "DELETE FROM entries WHERE id IN (SELECT id FROM entries WHERE event_ns < ",
                    "?1 OR collected_ns < ?1 LIMIT 1000)"
                ),
                [cutoff],
            )?;
            deleted += n;
            if n < 1000 {
                break;
            }
        }
        // Secure deletion is enabled; checkpoints may be deferred while another reader is active.
        self.db
            .execute_batch("PRAGMA incremental_vacuum(128); PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(deleted)
    }

    pub fn stats(&self) -> Result<(i64, Option<i64>)> {
        Ok(self
            .db
            .query_row("SELECT count(*), min(event_ns) FROM entries", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?)
    }

    pub fn search(
        &self,
        source: &str,
        text: &str,
        from: i64,
        until: i64,
        limit: usize,
    ) -> Result<Vec<SearchRow>> {
        let mut stmt = self.db.prepare(concat!(
            "SELECT event_ns,timestamp_origin,message FROM entries WHERE event_ns>=?1 ",
            "AND event_ns<?2 AND instr(message,?3)>0 ORDER BY event_ns DESC,id DESC ",
            "LIMIT ?4"
        ))?;
        let result = stmt
            .query_map(params![from, until, text, limit.min(1000) as i64], |row| {
                Ok(SearchRow {
                    source: source.into(),
                    timestamp: format_ns(row.get(0)?),
                    timestamp_origin: row.get(1)?,
                    message: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }

    /// Righe arrivate dopo la riga `after` (in ordine di arrivo) che contengono `text`,
    /// più l'identificativo da cui ripartire la volta successiva.
    pub fn rows_after(
        &self,
        source: &str,
        after: i64,
        text: &str,
        limit: usize,
    ) -> Result<(Vec<SearchRow>, i64)> {
        let last: i64 = self
            .db
            .query_row("SELECT coalesce(max(id), 0) FROM entries", [], |r| r.get(0))?;
        // Se la scadenza ha svuotato l'archivio, la numerazione ricomincia da capo.
        let after = if last < after { 0 } else { after };
        let mut stmt = self.db.prepare(
            "SELECT id, event_ns, timestamp_origin, message FROM entries \
             WHERE id > ?1 AND instr(message, ?2) > 0 ORDER BY id LIMIT ?3",
        )?;
        let mut next = last;
        let mut rows = Vec::new();
        let mut query = stmt.query(params![after, text, limit as i64])?;
        while let Some(row) = query.next()? {
            next = row.get(0)?;
            rows.push(SearchRow {
                source: source.into(),
                timestamp: format_ns(row.get(1)?),
                timestamp_origin: row.get(2)?,
                message: row.get(3)?,
            });
        }
        // Se il limite ha tagliato le righe, la prossima volta si riparte dall'ultima data.
        if rows.len() < limit {
            next = last;
        }
        Ok((rows, next))
    }

    /// Punti di lettura di file (anche non più presenti) arrivati oltre l'inizio.
    pub fn file_cursors(&self) -> Result<Vec<(String, i64, u64)>> {
        let mut stmt = self.db.prepare(
            "SELECT file_id, generation, byte_offset FROM cursors WHERE byte_offset > 0",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Impronta dei primi `upto` byte archiviati di un file, oppure `None` se
    /// l'archivio non li contiene tutti e in ordine (per esempio dopo una scadenza).
    pub fn archived_prefix_digest(
        &self,
        file_id: &str,
        generation: i64,
        upto: u64,
    ) -> Result<Option<String>> {
        let mut stmt = self.db.prepare(
            "SELECT byte_offset, raw FROM entries \
             WHERE file_id=?1 AND generation=?2 AND byte_offset < ?3 ORDER BY byte_offset",
        )?;
        let mut rows = stmt.query(params![file_id, generation, upto as i64])?;
        let mut offset = 0u64;
        let mut hash = Sha256::new();
        while let Some(row) = rows.next()? {
            let start: i64 = row.get(0)?;
            let raw: Vec<u8> = row.get(1)?;
            if start as u64 != offset {
                return Ok(None);
            }
            hash.update(&raw);
            offset += raw.len() as u64;
        }
        Ok((offset == upto).then(|| format!("{:x}", hash.finalize())))
    }

    /// Verify every byte, not merely the line count. Missing/expired entries fail closed.
    pub fn archived_file_digest(
        &self,
        file_id: &str,
        generation: i64,
        expected_size: u64,
    ) -> Result<String> {
        let mut stmt = self.db.prepare("SELECT byte_offset,raw FROM entries WHERE file_id=?1 AND generation=?2 ORDER BY byte_offset")?;
        let mut rows = stmt.query(params![file_id, generation])?;
        let mut offset = 0u64;
        let mut hash = Sha256::new();
        while let Some(row) = rows.next()? {
            let start: i64 = row.get(0)?;
            let raw: Vec<u8> = row.get(1)?;
            if start as u64 != offset {
                bail!("The archive does not contain all bytes of this file; original preserved");
            }
            hash.update(&raw);
            offset += raw.len() as u64;
        }
        if offset != expected_size {
            bail!("Archived size differs from original; original preserved");
        }
        Ok(format!("{:x}", hash.finalize()))
    }
}
