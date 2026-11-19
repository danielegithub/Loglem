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
const SCHEMA_VERSION: i64 = 5;

/// Versione 2: niente più copia identica di ogni riga (`raw`). Per ogni file già letto si
/// calcola la catena di impronte dalle copie che ci sono ancora, poi la colonna sparisce.
/// Tutto in una transazione: se qualcosa non torna, l'archivio resta com'era.
fn migrate_v1_to_v2(db: &Connection) -> Result<()> {
    db.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        // Un altro processo può aver migrato mentre aspettavamo il blocco.
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version >= 2 {
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
#[derive(Clone)]
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

/// Archivi già alla versione 4 creati quando `entry_total` non aveva ancora `first_ns`:
/// `metrics.sql` si applica solo sotto la versione 4, quindi nessuno li aggiornava e ogni
/// lettura dello stato falliva con «no such column: first_ns». Aggiunge la colonna, rifà i
/// trigger (quelli vecchi non la aggiornano) e la ricalcola. Non perde né cambia nessuna riga.
fn repair_entry_total(db: &Connection) -> Result<()> {
    let columns = |name: &str| -> Result<i64> {
        Ok(db.query_row(
            "SELECT count(*) FROM pragma_table_info('entry_total') WHERE name=?1",
            [name],
            |r| r.get(0),
        )?)
    };
    if columns("n")? == 0 || columns("first_ns")? > 0 {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    tx.execute_batch(
        "DROP TRIGGER IF EXISTS metrics_insert; \
         DROP TRIGGER IF EXISTS metrics_delete; \
         DROP TRIGGER IF EXISTS metrics_edit; \
         ALTER TABLE entry_total ADD COLUMN first_ns INTEGER;",
    )?;
    tx.execute_batch(include_str!("metrics.sql"))?;
    tx.execute(
        "UPDATE entry_total SET first_ns=(SELECT min(t) FROM \
         (SELECT min(event_ns) t FROM entries UNION ALL SELECT min(first_ns) FROM blocks))",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

/// Le chiavi di deduplicazione di journal, Podman e Kubernetes (72 byte di testo) si salvano
/// come i primi 16 byte della loro impronta SHA-256: la probabilità di una collisione è
/// trascurabile e l'archivio occupa molto meno.
fn register_key(db: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    use sha2::{Digest, Sha256};
    db.create_scalar_function(
        "loglem_key",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let text = ctx.get_raw(0).as_str()?;
            Ok(Sha256::digest(text.as_bytes())[..16].to_vec())
        },
    )?;
    Ok(())
}

/// Archivi con le chiavi in chiaro (schema 4): le porta alla forma compatta, senza perdere
/// nessuna chiave e senza l'indice che le copiava una seconda volta.
fn compact_block_keys(db: &Connection) -> Result<()> {
    let old: i64 = db.query_row(
        "SELECT count(*) FROM pragma_table_info('block_keys') WHERE name='key'",
        [],
        |r| r.get(0),
    )?;
    if old == 0 {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    tx.execute_batch(
        "DROP INDEX IF EXISTS keys_block; \
         ALTER TABLE block_keys RENAME TO block_keys_old; \
         CREATE TABLE block_keys(k BLOB PRIMARY KEY, block_id INTEGER NOT NULL) WITHOUT ROWID; \
         INSERT OR IGNORE INTO block_keys SELECT loglem_key(key), block_id FROM block_keys_old; \
         DROP TABLE block_keys_old;",
    )?;
    tx.commit()?;
    let mut vacuum = db.prepare("PRAGMA incremental_vacuum")?;
    let mut pages = vacuum.query([])?;
    while pages.next()?.is_some() {}
    Ok(())
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        private_dir(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Missing archive directory"))?,
        )?;
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        register_key(&db)?;
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
        } else if version == 1 {
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
        if version < 4 {
            let tx = db.unchecked_transaction()?;
            tx.execute_batch(include_str!("metrics.sql"))?;
            tx.execute_batch(include_str!("blocks.sql"))?;
            tx.execute(
                "INSERT OR IGNORE INTO state SELECT 'highest_id',coalesce(max(id),0) FROM entries",
                [],
            )?;
            tx.commit()?;
        }
        repair_entry_total(&db)?;
        compact_block_keys(&db)?;
        if version == 4 {
            db.execute_batch("PRAGMA user_version=5")?;
        }
        Self::with_log_rows(db)
    }

    /// Consultation opens existing archives without creating or migrating them.
    pub(crate) fn open_reader(path: &Path) -> Result<Self> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(Duration::from_millis(250))?;
        db.execute_batch("PRAGMA cache_size=-512; PRAGMA mmap_size=0")?;
        register_key(&db)?;
        Self::with_log_rows(db)
    }

    fn with_log_rows(db: Connection) -> Result<Self> {
        use rusqlite::functions::FunctionFlags;
        crate::formats::register(&db)?;
        db.create_scalar_function(
            "loglem_decode",
            1,
            FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                use std::io::Read;
                let input = ctx.get_raw(0).as_blob()?;
                let mut text = String::new();
                flate2::read::GzDecoder::new(input)
                    .take(crate::MAX_LINE_BYTES as u64 * 8 + 1)
                    .read_to_string(&mut text)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
                if text.len() > crate::MAX_LINE_BYTES * 8 {
                    return Err(rusqlite::Error::UserFunctionError(
                        "Riga compressa oltre il limite".into(),
                    ));
                }
                Ok(text)
            },
        )?;
        db.execute_batch("CREATE TEMP VIEW hot_rows AS SELECT id,record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,CASE typeof(message) WHEN 'blob' THEN loglem_decode(message) ELSE message END AS message FROM main.entries")?;
        if db.table_exists(None, "blocks")? {
            db.execute_batch(crate::blocks::VIEW)?;
        } else {
            db.execute_batch(
                "CREATE TEMP VIEW log_rows AS SELECT *,NULL AS block_id,id AS last_id,event_ns AS last_ns,event_ns AS first_ns FROM hot_rows",
            )?;
        }
        Ok(Self { db })
    }

    pub fn compress_closed(&self) -> Result<()> {
        crate::blocks::pack(&self.db, crate::timestamps::now_ns())
    }

    /// Vero se l'archivio è nel formato precedente e va migrato (non lo modifica).
    pub fn needs_migration(path: &Path) -> bool {
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|db| {
                let version = db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?;
                // Alla versione 4 ma con `entry_total` vecchia: va riparata prima di leggerla.
                let old_totals: i64 = db.query_row(
                    "SELECT (SELECT count(*) FROM pragma_table_info('entry_total') WHERE name='n') \
                     > 0 AND (SELECT count(*) FROM pragma_table_info('entry_total') WHERE name='first_ns') = 0",
                    [],
                    |r| r.get(0),
                )?;
                Ok(version > 0 && (version < SCHEMA_VERSION || old_totals == 1))
            })
            .unwrap_or(false)
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
        let mut last:i64=tx.query_row("SELECT max(coalesce((SELECT max(id) FROM entries),0),coalesce((SELECT CAST(value AS INTEGER) FROM state WHERE key='highest_id'),0))",[],|r|r.get(0))?;
        let mut insert=tx.prepare_cached("INSERT OR IGNORE INTO entries(id,record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,message) VALUES(?,?,?,?,?,?,?,?,?)")?;
        let mut seen=tx.prepare_cached("SELECT EXISTS(SELECT 1 FROM block_keys WHERE k=loglem_key(?1)) OR EXISTS(SELECT 1 FROM blocks b JOIN files f ON f.id=b.file_ref WHERE f.name=?2 AND b.generation=?3 AND b.last_offset>=?4 AND b.first_offset<=?4 AND EXISTS(SELECT 1 FROM json_each(loglem_decode(b.data)) j WHERE j.value->>1=?1))")?;
        for e in entries {
            if seen.query_row(params![e.key, e.file_id, e.generation, e.offset], |r| {
                r.get::<_, bool>(0)
            })? {
                continue;
            }
            let n = insert.execute(params![
                last + 1,
                e.key,
                e.file_id,
                e.generation,
                e.offset,
                e.event_ns,
                e.collected_ns,
                e.timestamp_origin,
                e.message
            ])?;
            last += n as i64;
            added += n;
        }
        tx.execute(
            "INSERT OR REPLACE INTO state VALUES('highest_id',?)",
            [last],
        )?;
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
        if added > 0 {
            self.compress_closed()?;
        }
        Ok(added)
    }

    pub fn save_entries(&mut self, entries: &[Entry]) -> Result<usize> {
        let tx = self.db.transaction()?;
        let added = Self::insert(&tx, entries)?;
        tx.commit()?;
        if added > 0 {
            self.compress_closed()?;
        }
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
        let mut deleted =
            crate::blocks::discard(&self.db, "last_ns<?1 OR last_col<?1", &[&cutoff])?;
        let thawed = crate::blocks::unpack(&self.db, "first_ns<?1 OR first_col<?1", &[&cutoff])?;
        // Delete in bounded transactions so retention cannot monopolize the archive.
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
        if thawed {
            crate::blocks::pack(&self.db, i64::MAX)?;
        }
        // Secure deletion is enabled; checkpoints may be deferred while another reader is active.
        self.db
            .execute_batch("PRAGMA incremental_vacuum(128); PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(deleted)
    }

    pub fn stats(&self) -> Result<(i64, Option<i64>)> {
        if !self.db.table_exists(None, "entry_total")? {
            return Ok(self.db.query_row(
                "SELECT count(*),min(event_ns) FROM entries",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?);
        }
        Ok(self
            .db
            .query_row("SELECT n,first_ns FROM entry_total", [], |r| {
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
        crate::blocks::search(&self.db, source, query, from, until, limit)
    }

    /// Quante righe dal momento `since` contengono `text` (per gli allarmi).
    pub fn count_since(&self, since: i64, text: &str) -> Result<i64> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM log_rows WHERE event_ns >= ?1 AND instr(message, ?2) > 0",
            params![since, text],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn pattern_hits(&self, since: i64, text: &str, minutes: u32) -> Result<i64> {
        let key = crate::util::digest(format!("{minutes}\0{text}").as_bytes());
        let tx = self.db.unchecked_transaction()?;
        let upto = tx
            .query_row("SELECT upto FROM alert_scan WHERE key=?", [&key], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .unwrap_or(0);
        let last: i64 = tx.query_row(
            "SELECT CAST(value AS INTEGER) FROM state WHERE key='highest_id'",
            [],
            |r| r.get(0),
        )?;
        if last > upto {
            tx.execute("INSERT OR IGNORE INTO alert_hits SELECT ?1,id,event_ns FROM log_rows WHERE last_id>?2 AND last_ns>=?3 AND id>?2 AND event_ns>=?3 AND instr(message,?4)>0",params![key,upto,since,text])?;
        }
        tx.execute(
            "DELETE FROM alert_hits WHERE key=? AND at<?",
            params![key, since],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO alert_scan VALUES(?,?)",
            params![key, last],
        )?;
        let count = tx.query_row("SELECT count(*) FROM alert_hits WHERE key=?", [key], |r| {
            r.get(0)
        })?;
        tx.commit()?;
        Ok(count)
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
            .query_row("SELECT max(coalesce((SELECT max(id) FROM entries),0),coalesce((SELECT CAST(value AS INTEGER) FROM state WHERE key='highest_id'),0))", [], |r| r.get(0))?;
        // Un archivio ripristinato può avere un ultimo id precedente al cursore del client.
        let after = if last < after { 0 } else { after };
        let (filters, mut values) = query.sql();
        let mut stmt = self.db.prepare(&format!(
            "SELECT id, event_ns, timestamp_origin, message FROM log_rows \
             WHERE last_id > ?1 AND id > ?1{filters} ORDER BY id LIMIT ?"
        ))?;
        values.insert(0, Value::Integer(after));
        values.push(Value::Integer(limit as i64));
        let mut next = last;
        let mut rows = Vec::new();
        let mut found = stmt.query(rusqlite::params_from_iter(values))?;
        while let Some(row) = found.next()? {
            next = row.get(0)?;
            let message: String = row.get(3)?;
            rows.push(SearchRow {
                source: source.into(),
                timestamp: format_ns(row.get(1)?),
                timestamp_origin: row.get(2)?,
                level: crate::query::severity(&message).name().to_owned(),
                message,
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
            let stored: String = r.get(0)?;
            // Un contenuto ritirato da un troncamento (`retired:<file>:<generazione>`) vale
            // come il file da cui viene: le sue righe sono salvate con quella identità.
            let file_id = stored
                .strip_prefix(RETIRED)
                .and_then(|rest| rest.rsplit_once(':'))
                .map_or(stored.clone(), |(id, _)| id.to_owned());
            Ok(FileCursor {
                file_id,
                generation: r.get(1)?,
                offset: r.get::<_, i64>(2)? as u64,
                chain: r.get(3)?,
                lines: r.get::<_, i64>(4)? as u64,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Un file è stato troncato (per esempio da `copytruncate` di logrotate): il suo vecchio
    /// contenuto resta riconoscibile, così una copia ruotata non viene importata di nuovo.
    pub fn retire_cursor(&self, file_id: &str, cursor: &Cursor) -> Result<()> {
        if cursor.offset == 0 || cursor.chain.is_empty() {
            return Ok(());
        }
        self.db.execute(
            "INSERT OR REPLACE INTO cursors(file_id, byte_offset, generation, anchor, chain, lines) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("{RETIRED}{file_id}:{}", cursor.generation),
                cursor.offset as i64,
                cursor.generation,
                cursor.anchor,
                cursor.chain,
                cursor.lines as i64
            ],
        )?;
        Ok(())
    }

    /// Dopo aver riscritto un file originale (tolte righe): il file ha una nuova identità e un
    /// nuovo contenuto. Le righe già archiviate passano alla nuova identità e il punto di lettura
    /// va in fondo al nuovo file, così non viene riletto come se fosse nuovo.
    pub fn rekey_file(&mut self, old_id: &str, new_id: &str, cursor: &Cursor) -> Result<()> {
        crate::blocks::unpack(
            &self.db,
            "file_ref=(SELECT id FROM files WHERE name=?)",
            &[&old_id],
        )?;
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
        let count = |id: &str| -> Result<u64> {
            Ok(self.db.query_row(
                "SELECT (SELECT count(*) FROM entries WHERE file_id=?1 AND generation=?2)+coalesce((SELECT sum(lines) FROM blocks WHERE file_ref=(SELECT id FROM files WHERE name=?1) AND generation=?2),0)",
                params![id, generation],
                |r| r.get::<_, i64>(0),
            )? as u64)
        };
        let own = count(file_id)?;
        if own > 0 {
            return Ok(own);
        }
        match self.get_state(&format!("{COPY_OF}{file_id}"))? {
            Some(original) => count(&original),
            None => Ok(0),
        }
    }
}

/// Prefisso dei punti di lettura di file che sono stati troncati.
const RETIRED: &str = "retired:";
/// Stato: questo file è una copia di un altro già archiviato (`copy_of:<file>` = `<originale>`).
pub const COPY_OF: &str = "copy_of:";

#[cfg(test)]
mod tests {
    use super::*;

    /// Un archivio com'era prima che `entry_total` avesse `first_ns`, con le sue righe.
    fn old_archive(path: &Path) {
        let archive = Archive::open(path).unwrap();
        for (i, ns) in [300_i64, 100, 200].iter().enumerate() {
            archive
                .db
                .execute(
                    "INSERT INTO entries(record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,message) \
                     VALUES(?1,'f',1,?2,?3,?3,'event','riga')",
                    rusqlite::params![format!("k{i}"), i as i64, ns],
                )
                .unwrap();
        }
        drop(archive);
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "DROP TRIGGER metrics_insert; DROP TRIGGER metrics_delete; DROP TRIGGER metrics_edit; \
             ALTER TABLE entry_total DROP COLUMN first_ns; \
             CREATE TRIGGER metrics_insert AFTER INSERT ON entries BEGIN UPDATE entry_total SET n=n+1; END; \
             CREATE TRIGGER metrics_delete AFTER DELETE ON entries BEGIN UPDATE entry_total SET n=n-1; END; \
             CREATE TRIGGER metrics_edit AFTER UPDATE OF message ON entries BEGIN SELECT 1; END;",
        )
        .unwrap();
        assert!(db
            .query_row("SELECT n,first_ns FROM entry_total", [], |r| r
                .get::<_, i64>(0))
            .is_err());
    }

    #[test]
    fn a_version_4_archive_without_first_ns_is_repaired_when_opened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        old_archive(&path);
        let archive = Archive::open(&path).unwrap();
        let (n, first): (i64, Option<i64>) = archive
            .db
            .query_row("SELECT n,first_ns FROM entry_total", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            (n, first),
            (3, Some(100)),
            "righe e data più vecchia ricalcolate"
        );
        // I trigger nuovi tengono i valori giusti dopo la riparazione.
        archive
            .db
            .execute(
                "INSERT INTO entries(record_key,file_id,generation,byte_offset,event_ns,collected_ns,timestamp_origin,message) VALUES('k9','f',1,9,50,50,'event','nuova')",
                [],
            )
            .unwrap();
        let (n, first): (i64, Option<i64>) = archive
            .db
            .query_row("SELECT n,first_ns FROM entry_total", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((n, first), (4, Some(50)));
        // Aprirlo di nuovo non cambia niente.
        drop(archive);
        let again = Archive::open(&path).unwrap();
        let n: i64 = again
            .db
            .query_row("SELECT n FROM entry_total", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);
    }
}
