use crate::query::Query;
use crate::{
    model::{Cursor, Entry, SearchRow},
    timestamps::format_ns,
    util::private_dir,
};
use anyhow::{bail, Result};
use rusqlite::{params, types::Value, Connection, OptionalExtension, Transaction};
use std::{path::Path, time::Duration};

const GAPS: &str = "gaps";
const SCHEMA_VERSION: i64 = 2;

/// Versione 2: niente più copia identica di ogni riga (`raw`). Per ogni file già letto si
/// calcola la catena di impronte dalle copie che ci sono ancora, poi la colonna sparisce.
/// Tutto in una transazione: se qualcosa non torna, l'archivio resta com'era.
fn migrate_v1_to_v2(db: &Connection) -> Result<()> {
    db.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        // Un altro processo può aver migrato mentre aspettavamo il blocco.
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        let rows_before: i64 = db.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
        db.execute_batch(
            "ALTER TABLE cursors ADD COLUMN chain TEXT NOT NULL DEFAULT '';
             ALTER TABLE cursors ADD COLUMN lines INTEGER NOT NULL DEFAULT 0;",
        )?;
        let cursors: Vec<(String, i64, i64)> = db
            .prepare("SELECT file_id, generation, byte_offset FROM cursors WHERE byte_offset > 0")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (file_id, generation, offset) in cursors {
            let mut select = db.prepare(
                "SELECT byte_offset, raw FROM entries WHERE file_id = ?1 AND generation = ?2 \
                 ORDER BY byte_offset",
            )?;
            let mut rows = select.query(params![file_id, generation])?;
            let (mut chain, mut lines, mut position) = (String::new(), 0i64, 0i64);
            let mut contiguous = true;
            while let Some(row) = rows.next()? {
                let start: i64 = row.get(0)?;
                let raw: Vec<u8> = row.get(1)?;
                if start != position {
                    contiguous = false; // righe scadute: la catena non si può ricostruire
                    break;
                }
                chain = crate::util::chain_next(&chain, &raw);
                position += raw.len() as i64;
                lines += 1;
            }
            drop(rows);
            if contiguous && position == offset {
                db.execute(
                    "UPDATE cursors SET chain = ?1, lines = ?2 WHERE file_id = ?3",
                    params![chain, lines, file_id],
                )?;
            }
        }
        db.execute_batch("ALTER TABLE entries DROP COLUMN raw")?;
        let rows_after: i64 = db.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
        if rows_before != rows_after {
            bail!("La migrazione dell'archivio ha cambiato il numero di righe: annullata");
        }
        db.execute_batch("PRAGMA user_version = 2")?;
        Ok(())
    })();
    match result {
        Ok(()) => db.execute_batch("COMMIT")?,
        Err(error) => {
            let _ = db.execute_batch("ROLLBACK");
            return Err(error);
        }
    }
    // Restituisce al disco lo spazio delle copie eliminate: `DROP COLUMN` lascia buchi nelle
    // pagine, quindi serve compattare il file (per un momento serve altrettanto spazio libero).
    // La migrazione è già confermata: se fallisce, l'archivio funziona lo stesso.
    let _ = db.execute_batch("VACUUM");
    Ok(())
}

/// Il punto di lettura salvato di un file.
pub struct FileCursor {
    pub file_id: String,
    pub generation: i64,
    pub offset: u64,
    pub chain: String,
    pub lines: u64,
}

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
        if version > SCHEMA_VERSION {
            bail!("Archive schema {version} needs a newer Loglem; no changes made");
        }
        if version == 0 {
            db.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {} COMMIT;",
                include_str!("schema.sql")
            ))?;
        } else if version < SCHEMA_VERSION {
            migrate_v1_to_v2(&db)?;
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

    /// Vero se l'archivio è nel formato precedente e va migrato (non lo modifica).
    pub fn needs_migration(path: &Path) -> bool {
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|db| db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)))
            .is_ok_and(|version| version > 0 && version < SCHEMA_VERSION)
    }

    pub fn cursor(&self, file_id: &str) -> Result<Cursor> {
        Ok(self
            .db
            .query_row(
                "SELECT byte_offset, generation, anchor, chain, lines FROM cursors WHERE file_id = ?1",
                [file_id],
                |row| {
                    Ok(Cursor {
                        offset: row.get::<_, i64>(0)? as u64,
                        generation: row.get(1)?,
                        anchor: row.get(2)?,
                        chain: row.get(3)?,
                        lines: row.get::<_, i64>(4)? as u64,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    fn insert(tx: &Transaction<'_>, entries: &[Entry]) -> Result<usize> {
        let mut added = 0;
        let mut statement = tx.prepare_cached(
            "INSERT OR IGNORE INTO entries(record_key, file_id, generation, byte_offset, \
             event_ns, collected_ns, timestamp_origin, message) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for entry in entries {
            added += statement.execute(params![
                entry.key,
                entry.file_id,
                entry.generation,
                entry.offset,
                entry.event_ns,
                entry.collected_ns,
                entry.timestamp_origin,
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
        tx.execute(
            "INSERT INTO cursors(file_id, byte_offset, generation, anchor, chain, lines) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(file_id) DO UPDATE SET \
             byte_offset = excluded.byte_offset, generation = excluded.generation, \
             anchor = excluded.anchor, chain = excluded.chain, lines = excluded.lines",
            params![
                file_id,
                cursor.offset as i64,
                cursor.generation,
                cursor.anchor,
                cursor.chain,
                cursor.lines as i64
            ],
        )?;
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
        query: &Query,
        from: i64,
        until: i64,
        limit: usize,
    ) -> Result<Vec<SearchRow>> {
        let (filters, mut values) = query.sql();
        let mut stmt = self.db.prepare(&format!(
            "SELECT event_ns, timestamp_origin, message FROM entries \
             WHERE event_ns >= ? AND event_ns < ?{filters} \
             ORDER BY event_ns DESC, id DESC LIMIT ?"
        ))?;
        values.insert(0, Value::Integer(from));
        values.insert(1, Value::Integer(until));
        values.push(Value::Integer(limit.min(1000) as i64));
        let result = stmt
            .query_map(rusqlite::params_from_iter(values), |row| {
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

    /// Quante righe dal momento `since` contengono `text` (per gli allarmi).
    pub fn count_since(&self, since: i64, text: &str) -> Result<i64> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM entries WHERE event_ns >= ?1 AND instr(message, ?2) > 0",
            params![since, text],
            |r| r.get(0),
        )?)
    }

    /// Registra una lacuna. Se continua quella precedente con lo stesso motivo,
    /// la allunga invece di aggiungerne un'altra. Restano le ultime 20.
    pub fn record_gap(&self, from_ns: i64, to_ns: i64, reason: &str) -> Result<()> {
        let mut gaps = self.gaps()?;
        match gaps.last_mut() {
            Some(last) if last.reason == reason && from_ns <= last.to_ns => {
                last.to_ns = last.to_ns.max(to_ns);
            }
            _ => gaps.push(crate::model::Gap {
                from_ns,
                to_ns,
                reason: reason.to_owned(),
            }),
        }
        let keep = gaps.len().saturating_sub(20);
        self.set_state(GAPS, &serde_json::to_string(&gaps[keep..])?)
    }

    pub fn gaps(&self) -> Result<Vec<crate::model::Gap>> {
        Ok(self
            .get_state(GAPS)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    /// Righe arrivate dopo la riga `after` (in ordine di arrivo) che contengono `text`,
    /// più l'identificativo da cui ripartire la volta successiva.
    pub fn rows_after(
        &self,
        source: &str,
        after: i64,
        query: &Query,
        limit: usize,
    ) -> Result<(Vec<SearchRow>, i64)> {
        let last: i64 = self
            .db
            .query_row("SELECT coalesce(max(id), 0) FROM entries", [], |r| r.get(0))?;
        // Se la scadenza ha svuotato l'archivio, la numerazione ricomincia da capo.
        let after = if last < after { 0 } else { after };
        let (filters, mut values) = query.sql();
        let mut stmt = self.db.prepare(&format!(
            "SELECT id, event_ns, timestamp_origin, message FROM entries \
             WHERE id > ?{filters} ORDER BY id LIMIT ?"
        ))?;
        values.insert(0, Value::Integer(after));
        values.push(Value::Integer(limit as i64));
        let mut next = last;
        let mut rows = Vec::new();
        let mut found = stmt.query(rusqlite::params_from_iter(values))?;
        while let Some(row) = found.next()? {
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

    /// Punti di lettura di file (anche non più presenti) di cui si conosce la catena.
    pub fn file_cursors(&self) -> Result<Vec<FileCursor>> {
        let mut stmt = self.db.prepare(
            "SELECT file_id, generation, byte_offset, chain, lines FROM cursors \
             WHERE byte_offset > 0 AND chain != ''",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(FileCursor {
                file_id: r.get(0)?,
                generation: r.get(1)?,
                offset: r.get::<_, i64>(2)? as u64,
                chain: r.get(3)?,
                lines: r.get::<_, i64>(4)? as u64,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Dopo aver riscritto un file originale (tolte righe): il file ha una nuova identità e un
    /// nuovo contenuto. Le righe già archiviate passano alla nuova identità e il punto di lettura
    /// va in fondo al nuovo file, così non viene riletto come se fosse nuovo.
    pub fn rekey_file(&mut self, old_id: &str, new_id: &str, cursor: &Cursor) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE entries SET file_id = ?1 WHERE file_id = ?2",
            params![new_id, old_id],
        )?;
        tx.execute("DELETE FROM cursors WHERE file_id = ?1", params![old_id])?;
        tx.execute(
            "INSERT OR REPLACE INTO cursors(file_id, byte_offset, generation, anchor, chain, lines) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                new_id,
                cursor.offset as i64,
                cursor.generation,
                cursor.anchor,
                cursor.chain,
                cursor.lines as i64
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Quante righe di questa lettura di file sono ancora nell'archivio (le scadute mancano).
    pub fn rows_of_file(&self, file_id: &str, generation: i64) -> Result<u64> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM entries WHERE file_id = ?1 AND generation = ?2",
            params![file_id, generation],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }
}
