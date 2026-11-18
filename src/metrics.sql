CREATE TABLE IF NOT EXISTS alert_scan(key TEXT PRIMARY KEY,upto INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS alert_hits(key TEXT NOT NULL,id INTEGER NOT NULL,at INTEGER NOT NULL,PRIMARY KEY(key,id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS alert_hits_id ON alert_hits(id);
CREATE INDEX IF NOT EXISTS alert_hits_time ON alert_hits(key,at);
CREATE TABLE IF NOT EXISTS entry_total(n INTEGER NOT NULL, first_ns INTEGER);
INSERT INTO entry_total SELECT count(*),min(event_ns) FROM entries HAVING NOT EXISTS(SELECT 1 FROM entry_total);
CREATE TABLE IF NOT EXISTS daily_stats(day INTEGER PRIMARY KEY, upto INTEGER NOT NULL DEFAULT 0, summary TEXT, dirty INTEGER NOT NULL DEFAULT 1);
INSERT OR IGNORE INTO daily_stats(day) SELECT DISTINCT (event_ns / 86400000000000 - (event_ns % 86400000000000 < 0)) FROM entries;
CREATE TRIGGER IF NOT EXISTS metrics_insert AFTER INSERT ON entries WHEN NOT EXISTS(SELECT 1 FROM state WHERE key='moving_block') BEGIN
 UPDATE entry_total SET n=n+1, first_ns=min(coalesce(first_ns,NEW.event_ns),NEW.event_ns);
 INSERT INTO daily_stats(day) VALUES((NEW.event_ns / 86400000000000 - (NEW.event_ns % 86400000000000 < 0))) ON CONFLICT(day) DO UPDATE SET dirty=1;
END;
CREATE TRIGGER IF NOT EXISTS metrics_delete AFTER DELETE ON entries WHEN NOT EXISTS(SELECT 1 FROM state WHERE key='moving_block') BEGIN
 DELETE FROM alert_hits WHERE id=OLD.id;
 UPDATE entry_total SET n=n-1, first_ns=CASE WHEN first_ns=OLD.event_ns THEN (SELECT min(t) FROM (SELECT min(event_ns) t FROM entries UNION ALL SELECT min(first_ns) FROM blocks)) ELSE first_ns END;
 UPDATE daily_stats SET upto=0, summary=NULL, dirty=1 WHERE day=(OLD.event_ns / 86400000000000 - (OLD.event_ns % 86400000000000 < 0));
END;
CREATE TRIGGER IF NOT EXISTS metrics_edit AFTER UPDATE OF message,event_ns ON entries WHEN NOT EXISTS(SELECT 1 FROM state WHERE key='moving_block') BEGIN
 DELETE FROM alert_hits WHERE id=OLD.id;
 UPDATE entry_total SET first_ns=(SELECT min(t) FROM (SELECT min(event_ns) t FROM entries UNION ALL SELECT min(first_ns) FROM blocks)) WHERE OLD.event_ns!=NEW.event_ns AND (first_ns>=NEW.event_ns OR first_ns=OLD.event_ns);
 UPDATE alert_scan SET upto=min(upto,OLD.id-1);
 UPDATE daily_stats SET upto=0, summary=NULL, dirty=1 WHERE day=(OLD.event_ns / 86400000000000 - (OLD.event_ns % 86400000000000 < 0));
 INSERT INTO daily_stats(day) VALUES((NEW.event_ns / 86400000000000 - (NEW.event_ns % 86400000000000 < 0))) ON CONFLICT(day) DO UPDATE SET upto=0, summary=NULL, dirty=1;
END;
