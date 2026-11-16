CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY,
    record_key TEXT NOT NULL UNIQUE,
    file_id TEXT,
    generation INTEGER NOT NULL,
    byte_offset INTEGER NOT NULL,
    event_ns INTEGER NOT NULL,
    collected_ns INTEGER NOT NULL,
    timestamp_origin TEXT NOT NULL CHECK(timestamp_origin IN ('event', 'collection')),
    raw BLOB NOT NULL,
    message TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_time ON entries(event_ns);
CREATE INDEX IF NOT EXISTS entries_collected ON entries(collected_ns);
CREATE INDEX IF NOT EXISTS entries_file ON entries(file_id, generation, byte_offset);
CREATE TABLE IF NOT EXISTS cursors (
    file_id TEXT PRIMARY KEY,
    byte_offset INTEGER NOT NULL,
    generation INTEGER NOT NULL,
    anchor TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
PRAGMA user_version = 1;

