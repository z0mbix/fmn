
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS commands (
    id INTEGER PRIMARY KEY,
    command TEXT NOT NULL UNIQUE,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    use_count INTEGER NOT NULL CHECK (use_count > 0)
);

CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY,
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    exit_code INTEGER,
    cwd TEXT NOT NULL,
    shell TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'live',
    import_key TEXT UNIQUE
);

CREATE INDEX IF NOT EXISTS entries_pending_session
    ON entries(session_id, finished_at, id DESC);
CREATE INDEX IF NOT EXISTS entries_command_time
    ON entries(command_id, started_at DESC);
CREATE INDEX IF NOT EXISTS entries_cwd_command_time
    ON entries(cwd, command_id, started_at DESC);

INSERT OR IGNORE INTO schema_migrations(version, applied_at)
VALUES (1, unixepoch('subsec') * 1000);
