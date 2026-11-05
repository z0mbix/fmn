package history

import (
	"context"
	"database/sql"
	"errors"
	"fmt"

	"modernc.org/sqlite"
	sqlite3 "modernc.org/sqlite/lib"
)

var ErrPurgePending = errors.New("history removed, but SQLite cleanup is pending; close other searches and run fmn history purge")

func sqliteBusy(err error) bool {
	var sqliteErr *sqlite.Error
	if !errors.As(err, &sqliteErr) {
		return false
	}
	code := sqliteErr.Code() & 255
	return code == sqlite3.SQLITE_BUSY || code == sqlite3.SQLITE_LOCKED
}

func (s *Store) Purge() error {
	ctx := context.Background()
	conn, err := s.db.Conn(ctx)
	if err != nil {
		return err
	}
	defer conn.Close()
	if _, err := conn.ExecContext(ctx, "PRAGMA busy_timeout = 0"); err != nil {
		return err
	}
	var busy, frames, checkpointed int
	checkErr := conn.QueryRowContext(ctx, "PRAGMA wal_checkpoint(TRUNCATE)").Scan(&busy, &frames, &checkpointed)
	if _, err := conn.ExecContext(ctx, "PRAGMA busy_timeout = 5000"); err != nil {
		return err
	}
	if busy != 0 || sqliteBusy(checkErr) {
		return ErrPurgePending
	}
	return checkErr
}

func (s *Store) DeleteEntry(id int64) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	var commandID int64
	var cwd string
	err = tx.QueryRow("SELECT command_id, cwd FROM entries WHERE id = ?", id).Scan(&commandID, &cwd)
	if errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("history entry %d was not found", id)
	}
	if err != nil {
		return err
	}
	if _, err := tx.Exec("DELETE FROM entries WHERE id = ?", id); err != nil {
		return err
	}
	if _, err := tx.Exec("DELETE FROM command_directories WHERE command_id = ? AND cwd = ?", commandID, cwd); err != nil {
		return err
	}
	if _, err := tx.Exec(`
		INSERT INTO command_directories(command_id, cwd, use_count, last_seen_at)
		SELECT command_id, cwd, COUNT(*), MAX(started_at) FROM entries
		WHERE command_id = ? AND cwd = ? GROUP BY command_id, cwd`, commandID, cwd); err != nil {
		return err
	}
	var count int64
	var first, last *int64
	if err := tx.QueryRow("SELECT COUNT(*), MIN(started_at), MAX(started_at) FROM entries WHERE command_id = ?", commandID).
		Scan(&count, &first, &last); err != nil {
		return err
	}
	if count == 0 {
		_, err = tx.Exec("DELETE FROM commands WHERE id = ?", commandID)
	} else {
		_, err = tx.Exec("UPDATE commands SET use_count = ?, first_seen_at = ?, last_seen_at = ? WHERE id = ?", count, first, last, commandID)
	}
	if err != nil {
		return err
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	return s.Purge()
}

func (s *Store) ForgetCommand(id int64) error {
	result, err := s.db.Exec("DELETE FROM commands WHERE id = ?", id)
	if err != nil {
		return err
	}
	n, err := result.RowsAffected()
	if err != nil {
		return err
	}
	if n == 0 {
		return fmt.Errorf("history command %d was not found", id)
	}
	return s.Purge()
}

func (s *Store) Clear() error {
	if _, err := s.db.Exec("DELETE FROM commands"); err != nil {
		return err
	}
	return s.Purge()
}
