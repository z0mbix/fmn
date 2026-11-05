
CREATE TABLE command_directories (
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    cwd TEXT NOT NULL,
    use_count INTEGER NOT NULL CHECK (use_count > 0),
    last_seen_at INTEGER NOT NULL,
    PRIMARY KEY (command_id, cwd)
) WITHOUT ROWID;
INSERT INTO command_directories(command_id, cwd, use_count, last_seen_at)
    SELECT command_id, cwd, COUNT(*), MAX(started_at) FROM entries GROUP BY command_id, cwd;
DROP INDEX entries_command_time;
CREATE INDEX entries_command_time ON entries(command_id, started_at DESC, id DESC);
CREATE INDEX commands_last_seen ON commands(last_seen_at DESC, id DESC);
INSERT INTO schema_migrations(version, applied_at)
VALUES (3, CAST(unixepoch('subsec') * 1000 AS INTEGER));
