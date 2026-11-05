
CREATE INDEX IF NOT EXISTS entries_time ON entries(started_at DESC, id DESC);
INSERT OR IGNORE INTO schema_migrations(version, applied_at)
VALUES (2, CAST(unixepoch('subsec') * 1000 AS INTEGER));
