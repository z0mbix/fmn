package importer

import (
	"crypto/sha256"
	"database/sql"
	"fmt"
	"net/url"
	"path/filepath"
	"unicode/utf8"

	"github.com/z0mbix/fmn/internal/history"
)

func loadAtuin(path string) (Result, error) {
	path, err := filepath.Abs(path)
	if err != nil {
		return Result{}, err
	}
	dsn := url.URL{Scheme: "file", Path: filepath.ToSlash(path), RawQuery: "mode=ro&_pragma=busy_timeout(5000)"}
	db, err := sql.Open("sqlite", dsn.String())
	if err != nil {
		return Result{}, err
	}
	defer db.Close()
	rows, err := db.Query("SELECT name FROM pragma_table_info('history')")
	if err != nil {
		return Result{}, err
	}
	columns := make(map[string]bool)
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			rows.Close()
			return Result{}, err
		}
		columns[name] = true
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return Result{}, err
	}
	for _, name := range []string{"id", "timestamp", "duration", "exit", "command", "cwd", "session"} {
		if !columns[name] {
			return Result{}, fmt.Errorf("unsupported Atuin database: history table is missing the %q column", name)
		}
	}
	shell, filter := "NULL", ""
	if columns["shell"] {
		shell = "shell"
	}
	if columns["deleted_at"] {
		filter = "WHERE deleted_at IS NULL"
	}
	rows, err = db.Query(fmt.Sprintf(`
		SELECT id, timestamp, duration, exit, command, cwd, %s, session
		FROM history %s ORDER BY timestamp, id`, shell, filter))
	if err != nil {
		return Result{}, err
	}
	defer rows.Close()
	var result Result
	for rows.Next() {
		var row history.ImportRecord
		var id, session string
		var timestamp, duration, exit int64
		if err := rows.Scan(&id, &timestamp, &duration, &exit, &row.Command, &row.Cwd, &row.Shell, &session); err != nil {
			return Result{}, err
		}
		if !utf8.ValidString(row.Command) {
			return Result{}, fmt.Errorf("Atuin record %q has an invalid UTF-8 command", id)
		}
		ms := timestamp / 1_000_000
		if timestamp < 0 && timestamp%1_000_000 != 0 {
			ms--
		}
		row.StartedAt = &ms
		if duration >= 0 {
			ms := duration / 1_000_000
			row.DurationMS = &ms
		}
		if int64(int32(exit)) == exit {
			code := int32(exit)
			row.ExitCode = &code
		} else {
			result.Warnings = append(result.Warnings, fmt.Sprintf("Atuin record %s has an out-of-range exit code", id))
		}
		row.SessionID = "atuin:" + session
		row.Key = fmt.Sprintf("%x", sha256.Sum256([]byte("atuin\x00"+id)))
		result.Records = append(result.Records, row)
	}
	return result, rows.Err()
}
