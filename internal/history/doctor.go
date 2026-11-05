package history

import (
	"fmt"
	"strings"
)

type Check struct {
	Status, Name, Detail string
}

func (s *Store) Checks() []Check {
	var checks []Check
	add := func(name, detail string, err error, status string) {
		if err != nil {
			status, detail = "error", err.Error()
		}
		checks = append(checks, Check{status, name, detail})
	}
	var value string
	err := s.db.QueryRow("PRAGMA integrity_check").Scan(&value)
	status := "ok"
	if value != "ok" {
		status = "error"
	}
	add("database integrity", value, err, status)
	version, err := currentSchema(s.db)
	status = "ok"
	detail := fmt.Sprint(version)
	if version != schemaVersion {
		status, detail = "error", fmt.Sprintf("unsupported version %d", version)
	}
	add("schema version", detail, err, status)
	err = s.db.QueryRow("PRAGMA journal_mode").Scan(&value)
	status = "ok"
	if !strings.EqualFold(value, "wal") {
		status, value = "warn", value+"; expected wal"
	}
	add("journal mode", value, err, status)
	var enabled bool
	err = s.db.QueryRow("PRAGMA foreign_keys").Scan(&enabled)
	status, value = "ok", "enabled"
	if !enabled {
		status, value = "error", "disabled"
	}
	add("foreign keys", value, err, status)
	return checks
}
