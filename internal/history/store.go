package history

import (
	"database/sql"
	"embed"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

const schemaVersion = 3

//go:embed schema/*.sql
var migrations embed.FS

type Store struct {
	db *sql.DB
}

type Entry struct {
	ID         int64  `json:"id"`
	CommandID  int64  `json:"command_id"`
	Command    string `json:"command"`
	StartedAt  int64  `json:"started_at"`
	FinishedAt *int64 `json:"finished_at"`
	DurationMS *int64 `json:"duration_ms"`
	ExitCode   *int32 `json:"exit_code"`
	Cwd        string `json:"cwd"`
	Shell      string `json:"shell"`
	SessionID  string `json:"session_id"`
	Source     string `json:"source"`
}

func Open(path string) (*Store, error) {
	path, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}
	parent := filepath.Dir(path)
	if err := os.MkdirAll(parent, 0o700); err != nil {
		return nil, err
	}
	if err := os.Chmod(parent, 0o700); err != nil {
		return nil, err
	}
	dsn := url.URL{
		Scheme:   "file",
		Path:     filepath.ToSlash(path),
		RawQuery: "_pragma=busy_timeout(5000)&_pragma=foreign_keys(ON)&_pragma=secure_delete(ON)&_pragma=synchronous(NORMAL)&_txlock=immediate",
	}
	db, err := sql.Open("sqlite", dsn.String())
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	store := &Store{db: db}
	deadline := time.Now().Add(5 * time.Second)
	for {
		err := store.prepare()
		if err == nil {
			return store, nil
		}
		if !sqliteBusy(err) || !time.Now().Before(deadline) {
			db.Close()
			return nil, fmt.Errorf("open history: %w", err)
		}
		// SQLite can report BUSY immediately when two first opens try to
		// switch to WAL. Recheck the schema and journal after the other open.
		time.Sleep(15 * time.Millisecond)
	}
}

func (s *Store) Close() error { return s.db.Close() }

type rowQuerier interface {
	QueryRow(string, ...any) *sql.Row
}

func currentSchema(db rowQuerier) (int, error) {
	var exists bool
	err := db.QueryRow("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')").Scan(&exists)
	if err != nil || !exists {
		return 0, err
	}
	var version int
	if err := db.QueryRow("SELECT COALESCE(MAX(version), 0) FROM schema_migrations").Scan(&version); err != nil {
		return 0, err
	}
	if version < 0 || version > schemaVersion {
		return 0, fmt.Errorf("unsupported schema version %d (supports up to %d)", version, schemaVersion)
	}
	return version, nil
}

func (s *Store) prepare() error {
	version, err := currentSchema(s.db)
	if err != nil {
		return err
	}
	var journal string
	if err := s.db.QueryRow("PRAGMA journal_mode").Scan(&journal); err != nil {
		return err
	}
	if !strings.EqualFold(journal, "wal") {
		if err := s.db.QueryRow("PRAGMA journal_mode = WAL").Scan(&journal); err != nil {
			return err
		}
		if !strings.EqualFold(journal, "wal") {
			return fmt.Errorf("could not enable WAL (got %s)", journal)
		}
	}
	if version == schemaVersion {
		return nil
	}

	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	// Another capture process may have migrated while this one waited for the lock.
	version, err = currentSchema(tx)
	if err != nil {
		return err
	}
	for next := version + 1; next <= schemaVersion; next++ {
		script, err := migrations.ReadFile("schema/" + strconv.Itoa(next) + ".sql")
		if err != nil {
			return err
		}
		if _, err := tx.Exec(string(script)); err != nil {
			return fmt.Errorf("migrate schema to %d: %w", next, err)
		}
	}
	return tx.Commit()
}

func (s *Store) RecordStart(command, cwd, shell, session string, startedAt int64) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	id, err := upsertCommand(tx, command, startedAt)
	if err != nil {
		return err
	}
	if _, err := tx.Exec(`
		INSERT INTO entries(command_id, started_at, cwd, shell, session_id, source)
		VALUES (?, ?, ?, ?, ?, 'live')`, id, startedAt, cwd, shell, session); err != nil {
		return err
	}
	if err := upsertDirectory(tx, id, cwd, startedAt); err != nil {
		return err
	}
	return tx.Commit()
}

func upsertCommand(tx *sql.Tx, command string, startedAt int64) (int64, error) {
	var id int64
	err := tx.QueryRow(`
		INSERT INTO commands(command, first_seen_at, last_seen_at, use_count)
		VALUES (?1, ?2, ?2, 1)
		ON CONFLICT(command) DO UPDATE SET
			first_seen_at = MIN(first_seen_at, excluded.first_seen_at),
			last_seen_at = MAX(last_seen_at, excluded.last_seen_at),
			use_count = use_count + 1
		RETURNING id`, command, startedAt).Scan(&id)
	return id, err
}

func upsertDirectory(tx *sql.Tx, id int64, cwd string, startedAt int64) error {
	_, err := tx.Exec(`
		INSERT INTO command_directories(command_id, cwd, use_count, last_seen_at)
		VALUES (?, ?, 1, ?)
		ON CONFLICT(command_id, cwd) DO UPDATE SET
			use_count = use_count + 1,
			last_seen_at = MAX(last_seen_at, excluded.last_seen_at)`, id, cwd, startedAt)
	return err
}

func (s *Store) RecordFinish(session string, finishedAt int64, exitCode int32) error {
	_, err := s.db.Exec(`
		UPDATE entries SET finished_at = ?1,
			duration_ms = MAX(0, MIN(9223372036854775807, ?1 - started_at)),
			exit_code = ?2
		WHERE id = (
			SELECT id FROM entries
			WHERE session_id = ?3 AND finished_at IS NULL
			ORDER BY id DESC LIMIT 1
		)`, finishedAt, exitCode, session)
	return err
}

func (s *Store) Entries(limit int64) ([]Entry, error) {
	rows, err := s.db.Query(`
		SELECT e.id, e.command_id, c.command, e.started_at, e.finished_at,
		       e.duration_ms, e.exit_code, e.cwd, e.shell, e.session_id, e.source
		FROM entries e JOIN commands c ON c.id = e.command_id
		ORDER BY e.started_at DESC, e.id DESC LIMIT ?`, limit)
	if err != nil {
		return nil, err
	}
	return scanEntries(rows)
}

func scanEntries(rows *sql.Rows) ([]Entry, error) {
	defer rows.Close()
	entries := make([]Entry, 0)
	for rows.Next() {
		var e Entry
		if err := rows.Scan(&e.ID, &e.CommandID, &e.Command, &e.StartedAt, &e.FinishedAt,
			&e.DurationMS, &e.ExitCode, &e.Cwd, &e.Shell, &e.SessionID, &e.Source); err != nil {
			return nil, err
		}
		entries = append(entries, e)
	}
	return entries, rows.Err()
}
