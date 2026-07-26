use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::import::ImportedRecord;
use crate::model::{CommandSummary, Entry, SearchCandidate, Shell};
use crate::{Error, Result};

const SCHEMA: &str = r#"
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
"#;

#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::configure(&connection, true)?;
        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::configure(&connection, false)?;
        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn configure(connection: &Connection, on_disk: bool) -> Result<()> {
        connection.busy_timeout(Duration::from_millis(500))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "secure_delete", "ON")?;
        if on_disk {
            let journal_mode: String =
                connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            if !journal_mode.eq_ignore_ascii_case("wal") {
                connection.pragma_update(None, "journal_mode", "WAL")?;
            }
            connection.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(())
    }

    fn migrate(&mut self) -> Result<()> {
        let migrations_exist: bool = self.connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = 'schema_migrations'
             )",
            [],
            |row| row.get(0),
        )?;
        if migrations_exist {
            let version = self.schema_version()?;
            if version >= 1 {
                return Ok(());
            }
        }
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(SCHEMA)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn record_start(
        &mut self,
        command: &str,
        started_at: i64,
        cwd: &str,
        shell: Shell,
        session_id: &str,
    ) -> Result<i64> {
        let transaction = self.connection.transaction()?;
        let command_id = upsert_command(&transaction, command, started_at)?;
        transaction.execute(
            "INSERT INTO entries(
                command_id, started_at, cwd, shell, session_id, source
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'live')",
            params![command_id, started_at, cwd, shell.as_str(), session_id],
        )?;
        let entry_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(entry_id)
    }

    pub fn record_finish(
        &mut self,
        session_id: &str,
        finished_at: i64,
        exit_code: i32,
    ) -> Result<Option<i64>> {
        let entry = self
            .connection
            .query_row(
                "SELECT id, started_at
                 FROM entries
                 WHERE session_id = ?1 AND finished_at IS NULL
                 ORDER BY id DESC
                 LIMIT 1",
                [session_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((entry_id, started_at)) = entry else {
            return Ok(None);
        };
        let duration_ms = finished_at.saturating_sub(started_at).max(0);
        self.connection.execute(
            "UPDATE entries
             SET finished_at = ?1, duration_ms = ?2, exit_code = ?3
             WHERE id = ?4 AND finished_at IS NULL",
            params![finished_at, duration_ms, exit_code, entry_id],
        )?;
        Ok(Some(entry_id))
    }

    pub fn entries(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(
            "SELECT e.id, e.command_id, c.command, e.started_at, e.finished_at,
                    e.duration_ms, e.exit_code, e.cwd, e.shell, e.session_id, e.source
             FROM entries e
             JOIN commands c ON c.id = e.command_id
             ORDER BY e.started_at DESC, e.id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], map_entry)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn commands(&self) -> Result<Vec<CommandSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, command, first_seen_at, last_seen_at, use_count
             FROM commands
             ORDER BY last_seen_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CommandSummary {
                id: row.get(0)?,
                command: row.get(1)?,
                first_seen_at: row.get(2)?,
                last_seen_at: row.get(3)?,
                use_count: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn search_candidates(&self, cwd: &str) -> Result<Vec<SearchCandidate>> {
        let mut statement = self.connection.prepare(
            "SELECT
                c.id,
                c.command,
                c.first_seen_at,
                c.last_seen_at,
                c.use_count,
                COALESCE(SUM(CASE WHEN e.cwd = ?1 THEN 1 ELSE 0 END), 0),
                MAX(CASE WHEN e.cwd = ?1 THEN e.started_at END),
                (
                    SELECT newest.cwd FROM entries newest
                    WHERE newest.command_id = c.id
                    ORDER BY newest.started_at DESC, newest.id DESC LIMIT 1
                ),
                (
                    SELECT newest.exit_code FROM entries newest
                    WHERE newest.command_id = c.id
                    ORDER BY newest.started_at DESC, newest.id DESC LIMIT 1
                )
             FROM commands c
             JOIN entries e ON e.command_id = c.id
             GROUP BY c.id
             ORDER BY c.last_seen_at DESC, c.id DESC",
        )?;
        let rows = statement.query_map([cwd], |row| {
            Ok(SearchCandidate {
                id: row.get(0)?,
                command: row.get(1)?,
                first_seen_at: row.get(2)?,
                last_seen_at: row.get(3)?,
                use_count: row.get(4)?,
                cwd_use_count: row.get(5)?,
                cwd_last_seen_at: row.get(6)?,
                latest_cwd: row.get(7)?,
                latest_exit_code: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn entries_for_command(&self, command_id: i64) -> Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(
            "SELECT e.id, e.command_id, c.command, e.started_at, e.finished_at,
                    e.duration_ms, e.exit_code, e.cwd, e.shell, e.session_id, e.source
             FROM entries e
             JOIN commands c ON c.id = e.command_id
             WHERE e.command_id = ?1
             ORDER BY e.started_at DESC, e.id DESC",
        )?;
        let rows = statement.query_map([command_id], map_entry)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn delete_entry(&mut self, entry_id: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let command_id = transaction
            .query_row(
                "SELECT command_id FROM entries WHERE id = ?1",
                [entry_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or(Error::EntryNotFound(entry_id))?;
        transaction.execute("DELETE FROM entries WHERE id = ?1", [entry_id])?;
        refresh_or_delete_command(&transaction, command_id)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn forget_command(&mut self, command_id: i64) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM commands WHERE id = ?1", [command_id])?;
        if changed == 0 {
            return Err(Error::CommandNotFound(command_id));
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM entries", [])?;
        transaction.execute("DELETE FROM commands", [])?;
        transaction.commit()?;
        let _: (i64, i64, i64) =
            self.connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        Ok(())
    }

    pub fn integrity_check(&self) -> Result<String> {
        self.connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn journal_mode(&self) -> Result<String> {
        self.connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn foreign_keys_enabled(&self) -> Result<bool> {
        self.connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn has_import_key(&self, import_key: &str) -> Result<bool> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE import_key = ?1)",
                [import_key],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn import_record(
        &mut self,
        record: &ImportedRecord,
        import_source: &str,
        fallback_started_at: i64,
    ) -> Result<bool> {
        let transaction = self.connection.transaction()?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE import_key = ?1)",
            [&record.import_key],
            |row| row.get(0),
        )?;
        if exists {
            transaction.rollback()?;
            return Ok(false);
        }

        let started_at = record.started_at.unwrap_or(fallback_started_at);
        let command_id = upsert_command(&transaction, &record.command, started_at)?;
        let finished_at = record
            .duration_ms
            .and_then(|duration| started_at.checked_add(duration));
        let shell = record.shell.as_deref().unwrap_or(import_source);
        let cwd = record.cwd.as_deref().unwrap_or("");
        let session_id = record
            .session_id
            .clone()
            .unwrap_or_else(|| format!("import:{import_source}"));
        transaction.execute(
            "INSERT INTO entries(
                command_id, started_at, finished_at, duration_ms, exit_code, cwd,
                shell, session_id, source, import_key
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'import', ?9)",
            params![
                command_id,
                started_at,
                finished_at,
                record.duration_ms,
                record.exit_code,
                cwd,
                shell,
                session_id,
                record.import_key,
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }
}

fn upsert_command(transaction: &Transaction<'_>, command: &str, at: i64) -> Result<i64> {
    transaction.execute(
        "INSERT INTO commands(command, first_seen_at, last_seen_at, use_count)
         VALUES (?1, ?2, ?2, 1)
         ON CONFLICT(command) DO UPDATE SET
             first_seen_at = MIN(first_seen_at, excluded.first_seen_at),
             last_seen_at = MAX(last_seen_at, excluded.last_seen_at),
             use_count = use_count + 1",
        params![command, at],
    )?;
    transaction
        .query_row(
            "SELECT id FROM commands WHERE command = ?1",
            [command],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn refresh_or_delete_command(transaction: &Transaction<'_>, command_id: i64) -> Result<()> {
    let aggregate = transaction.query_row(
        "SELECT COUNT(*), MIN(started_at), MAX(started_at)
             FROM entries WHERE command_id = ?1",
        [command_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    if aggregate.0 == 0 {
        transaction.execute("DELETE FROM commands WHERE id = ?1", [command_id])?;
    } else {
        transaction.execute(
            "UPDATE commands
             SET use_count = ?1, first_seen_at = ?2, last_seen_at = ?3
             WHERE id = ?4",
            params![
                aggregate.0,
                aggregate.1.expect("nonempty aggregate"),
                aggregate.2.expect("nonempty aggregate"),
                command_id
            ],
        )?;
    }
    Ok(())
}

fn map_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        command_id: row.get(1)?,
        command: row.get(2)?,
        started_at: row.get(3)?,
        finished_at: row.get(4)?,
        duration_ms: row.get(5)?,
        exit_code: row.get(6)?,
        cwd: row.get(7)?,
        shell: row.get(8)?,
        session_id: row.get(9)?,
        source: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::Error;
    use crate::model::Shell;

    #[test]
    fn migrates_fresh_database() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        assert_eq!(store.integrity_check().unwrap(), "ok");
    }

    #[test]
    fn records_and_finishes_an_entry() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .record_start("echo hello", 1_000, "/tmp", Shell::Fish, "session")
            .unwrap();
        assert_eq!(store.record_finish("session", 1_025, 7).unwrap(), Some(id));
        let entries = store.entries(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].duration_ms, Some(25));
        assert_eq!(entries[0].exit_code, Some(7));
    }

    #[test]
    fn finish_without_pending_entry_is_a_noop() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(store.record_finish("missing", 100, 0).unwrap(), None);
    }

    #[test]
    fn repeated_commands_are_aggregated_but_occurrences_remain() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .record_start("git status", 10, "/one", Shell::Zsh, "a")
            .unwrap();
        store
            .record_start("git status", 20, "/two", Shell::Zsh, "b")
            .unwrap();
        let commands = store.commands().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].use_count, 2);
        assert_eq!(commands[0].first_seen_at, 10);
        assert_eq!(commands[0].last_seen_at, 20);
        assert_eq!(store.entries(10).unwrap().len(), 2);
    }

    #[test]
    fn deleting_occurrence_repairs_aggregates() {
        let mut store = Store::open_in_memory().unwrap();
        let first = store
            .record_start("ls", 10, "/one", Shell::Bash, "a")
            .unwrap();
        store
            .record_start("ls", 20, "/two", Shell::Bash, "b")
            .unwrap();
        store.delete_entry(first).unwrap();
        let commands = store.commands().unwrap();
        assert_eq!(commands[0].use_count, 1);
        assert_eq!(commands[0].first_seen_at, 20);
    }

    #[test]
    fn deleting_last_occurrence_removes_command() {
        let mut store = Store::open_in_memory().unwrap();
        let entry = store
            .record_start("pwd", 10, "/", Shell::Fish, "a")
            .unwrap();
        store.delete_entry(entry).unwrap();
        assert!(store.commands().unwrap().is_empty());
        assert!(matches!(
            store.delete_entry(entry),
            Err(Error::EntryNotFound(_))
        ));
    }

    #[test]
    fn forget_cascades_all_occurrences() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .record_start("pwd", 10, "/", Shell::Fish, "a")
            .unwrap();
        let command_id = store.commands().unwrap()[0].id;
        store.forget_command(command_id).unwrap();
        assert!(store.entries(10).unwrap().is_empty());
        assert!(matches!(
            store.forget_command(command_id),
            Err(Error::CommandNotFound(_))
        ));
    }

    #[test]
    fn clear_removes_everything() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .record_start("one", 10, "/", Shell::Fish, "a")
            .unwrap();
        store
            .record_start("two", 11, "/", Shell::Fish, "a")
            .unwrap();
        store.clear().unwrap();
        assert!(store.entries(10).unwrap().is_empty());
        assert!(store.commands().unwrap().is_empty());
    }

    #[test]
    fn negative_clock_adjustment_saturates_duration() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .record_start("date", 100, "/", Shell::Bash, "a")
            .unwrap();
        store.record_finish("a", 50, 0).unwrap();
        assert_eq!(store.entries(1).unwrap()[0].duration_ms, Some(0));
    }

    #[test]
    fn search_candidates_include_global_and_cwd_metadata() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .record_start("git status", 10, "/repo", Shell::Fish, "a")
            .unwrap();
        store.record_finish("a", 11, 0).unwrap();
        store
            .record_start("git status", 20, "/other", Shell::Fish, "b")
            .unwrap();
        store.record_finish("b", 21, 1).unwrap();
        let candidates = store.search_candidates("/repo").unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].use_count, 2);
        assert_eq!(candidates[0].cwd_use_count, 1);
        assert_eq!(candidates[0].cwd_last_seen_at, Some(10));
        assert_eq!(candidates[0].latest_cwd, "/other");
        assert_eq!(candidates[0].latest_exit_code, Some(1));
        assert_eq!(
            store.entries_for_command(candidates[0].id).unwrap().len(),
            2
        );
    }

    #[test]
    fn import_is_idempotent_and_preserves_duplicate_occurrences() {
        let parsed = crate::import::parse(Shell::Bash, "echo same\necho same\n");
        let mut store = Store::open_in_memory().unwrap();
        for (index, record) in parsed.records.iter().enumerate() {
            assert!(
                store
                    .import_record(record, Shell::Bash.as_str(), index as i64)
                    .unwrap()
            );
        }
        for record in &parsed.records {
            assert!(
                !store
                    .import_record(record, Shell::Bash.as_str(), 100)
                    .unwrap()
            );
            assert!(store.has_import_key(&record.import_key).unwrap());
        }
        assert_eq!(store.entries(10).unwrap().len(), 2);
        assert_eq!(store.commands().unwrap()[0].use_count, 2);
        assert_eq!(store.entries(1).unwrap()[0].source, "import");
    }
}
