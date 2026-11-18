CREATE INDEX IF NOT EXISTS entries_group_order ON entries(file_id,generation,id);
CREATE TABLE IF NOT EXISTS files(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS blocks(
 id INTEGER PRIMARY KEY, file_ref INTEGER, generation INTEGER NOT NULL,
 first_id INTEGER NOT NULL,last_id INTEGER NOT NULL,first_ns INTEGER NOT NULL,last_ns INTEGER NOT NULL,
 first_col INTEGER NOT NULL,last_col INTEGER NOT NULL,first_offset INTEGER NOT NULL,last_offset INTEGER NOT NULL,
 lines INTEGER NOT NULL,data BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS blocks_offsets ON blocks(file_ref,generation,last_offset);
CREATE INDEX IF NOT EXISTS blocks_time ON blocks(last_ns);
CREATE INDEX IF NOT EXISTS blocks_first ON blocks(first_ns);
CREATE TABLE IF NOT EXISTS block_keys(key TEXT PRIMARY KEY, block_id INTEGER NOT NULL) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS keys_block ON block_keys(block_id);
PRAGMA user_version=4;
