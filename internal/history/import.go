package history

import (
	"math"
	"strings"
)

type ImportRecord struct {
	Command, Cwd, SessionID, Key string
	StartedAt, DurationMS        *int64
	ExitCode                     *int32
	Shell                        *string
}

func hasImportKey(db rowQuerier, key string) (bool, error) {
	var exists bool
	err := db.QueryRow("SELECT EXISTS(SELECT 1 FROM entries WHERE import_key = ?)", key).Scan(&exists)
	return exists, err
}

func (s *Store) HasImportKey(key string) (bool, error) {
	return hasImportKey(s.db, key)
}

func (s *Store) Import(record ImportRecord, source string, fallback int64) (bool, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return false, err
	}
	defer tx.Rollback()
	if exists, err := hasImportKey(tx, record.Key); exists || err != nil {
		return false, err
	}

	stamp := fallback
	if record.StartedAt != nil {
		stamp = *record.StartedAt
	}
	var finished *int64
	if record.DurationMS != nil {
		elapsed := *record.DurationMS
		if (elapsed >= 0 && stamp <= math.MaxInt64-elapsed) || (elapsed < 0 && stamp >= math.MinInt64-elapsed) {
			at := stamp + elapsed
			finished = &at
		}
	}
	name, session := source, record.SessionID
	if record.Shell != nil && strings.TrimSpace(*record.Shell) != "" {
		name = *record.Shell
	}
	if session == "" {
		session = "import:" + source
	}
	id, err := upsertCommand(tx, record.Command, stamp)
	if err != nil {
		return false, err
	}
	if _, err := tx.Exec(`
		INSERT INTO entries(command_id,started_at,finished_at,duration_ms,exit_code,cwd,shell,session_id,source,import_key)
		VALUES(?,?,?,?,?,?,?,?,'import',?)`, id, stamp, finished, record.DurationMS, record.ExitCode, record.Cwd, name, session, record.Key); err != nil {
		return false, err
	}
	if err := upsertDirectory(tx, id, record.Cwd, stamp); err != nil {
		return false, err
	}
	if err := tx.Commit(); err != nil {
		return false, err
	}
	return true, nil
}
