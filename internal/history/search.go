package history

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

type Scope string

const (
	Global     Scope = "global"
	Directory  Scope = "directory"
	Repository Scope = "repository"
	Session    Scope = "session"
)

var scopes = [...]Scope{Global, Directory, Repository, Session}

func (s Scope) Label() string {
	switch s {
	case Directory:
		return "This directory"
	case Repository:
		return "This repository"
	case Session:
		return "This terminal session"
	default:
		return "All directories"
	}
}

type SearchContext struct {
	Cwd          string
	Repo         string
	PhysicalRepo string
	SessionID    string
}

func DetectSearchContext(cwd string) SearchContext {
	context := SearchContext{Cwd: cwd, SessionID: os.Getenv("FMN_SESSION_ID")}
	command := exec.Command("git", "-C", cwd, "rev-parse", "--show-toplevel")
	for _, variable := range os.Environ() {
		key, _, _ := strings.Cut(variable, "=")
		switch key {
		case "GIT_DIR", "GIT_WORK_TREE", "GIT_OPTIONAL_LOCKS":
		default:
			command.Env = append(command.Env, variable)
		}
	}
	command.Env = append(command.Env, "GIT_OPTIONAL_LOCKS=0")
	out, err := command.Output()
	if err != nil {
		return context
	}
	root := strings.TrimRight(string(out), "\r\n")
	if root == "" {
		return context
	}
	context.Repo, context.PhysicalRepo = root, root
	if physical, err := filepath.EvalSymlinks(root); err == nil {
		context.PhysicalRepo = physical
	}
	for dir := cwd; ; dir = filepath.Dir(dir) {
		if physical, err := filepath.EvalSymlinks(dir); err == nil && physical == context.PhysicalRepo {
			context.Repo = dir
			break
		}
		if filepath.Dir(dir) == dir {
			break
		}
	}
	return context
}

func (c SearchContext) Effective(scope Scope) Scope {
	switch scope {
	case Directory:
		return scope
	case Repository:
		if c.Repo != "" {
			return scope
		}
	case Session:
		if c.SessionID != "" {
			return scope
		}
	}
	return Global
}

func (c SearchContext) Next(scope Scope, backwards bool) Scope {
	for index, candidate := range scopes {
		if scope != candidate {
			continue
		}
		for step := 1; step <= len(scopes); step++ {
			offset := step
			if backwards {
				offset = -step
			}
			next := scopes[(index+offset+len(scopes))%len(scopes)]
			if c.Effective(next) == next {
				return next
			}
		}
	}
	return Global
}

type Candidate struct {
	ID         int64
	Command    string
	LastSeenAt int64
	UseCount   int64
	CwdUses    int64
	LatestCwd  string
	ExitCode   *int32
}

func (c SearchContext) predicate(scope Scope) (string, []any) {
	switch c.Effective(scope) {
	case Directory:
		return "cwd = ?1", []any{c.Cwd}
	case Repository:
		return `(cwd = ?2 OR substr(cwd, 1, length(rtrim(?2, '/')) + 1) = rtrim(?2, '/') || '/'
			OR cwd = ?3 OR substr(cwd, 1, length(rtrim(?3, '/')) + 1) = rtrim(?3, '/') || '/')`,
			[]any{c.Cwd, c.Repo, c.PhysicalRepo}
	case Session:
		return "session_id = ?2 AND source = 'live'", []any{c.Cwd, c.SessionID}
	default:
		return "?1 IS NOT NULL", []any{c.Cwd}
	}
}

func (s *Store) Occurrences(id int64, limit int, c SearchContext, scope Scope) ([]Entry, error) {
	filter, args := c.predicate(scope)
	query := fmt.Sprintf(`
		SELECT e.id, e.command_id, c.command, e.started_at, e.finished_at,
		       e.duration_ms, e.exit_code, e.cwd, e.shell, e.session_id, e.source
		FROM entries e JOIN commands c ON c.id = e.command_id
		WHERE %s AND e.command_id = ?%d
		ORDER BY e.started_at DESC, e.id DESC LIMIT ?%d`, filter, len(args)+1, len(args)+2)
	rows, err := s.db.Query(query, append(args, id, limit)...)
	if err != nil {
		return nil, err
	}
	return scanEntries(rows)
}

func (s *Store) Candidates(c SearchContext, scope Scope) ([]Candidate, error) {
	const global = `
		SELECT c.id, c.command, c.last_seen_at, c.use_count,
		       COALESCE(d.use_count, 0), newest.cwd, newest.exit_code
		FROM commands c
		LEFT JOIN command_directories d ON d.command_id = c.id AND d.cwd = ?1
		JOIN entries newest ON newest.id = (
			SELECT id FROM entries WHERE command_id = c.id
			ORDER BY started_at DESC, id DESC LIMIT 1
		)`
	scope = c.Effective(scope)
	query, args := global, []any{c.Cwd}
	if scope != Global {
		// Only fixed predicates enter the SQL. Paths and session IDs are bound.
		var filter string
		filter, args = c.predicate(scope)
		query = fmt.Sprintf(`
			WITH scoped AS (SELECT * FROM entries WHERE %s),
			summary AS (
				SELECT command_id, MAX(started_at) last_seen, COUNT(*) uses,
				       SUM(CASE WHEN cwd = ?1 THEN 1 ELSE 0 END) cwd_uses
				FROM scoped GROUP BY command_id
			)
			SELECT c.id, c.command, s.last_seen, s.uses, s.cwd_uses, newest.cwd, newest.exit_code
			FROM summary s JOIN commands c ON c.id = s.command_id
			JOIN scoped newest ON newest.id = (
				SELECT id FROM scoped WHERE command_id = c.id
				ORDER BY started_at DESC, id DESC LIMIT 1
			)`, filter)
	}
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var candidates []Candidate
	for rows.Next() {
		var item Candidate
		if err := rows.Scan(&item.ID, &item.Command, &item.LastSeenAt, &item.UseCount,
			&item.CwdUses, &item.LatestCwd, &item.ExitCode); err != nil {
			return nil, err
		}
		candidates = append(candidates, item)
	}
	return candidates, rows.Err()
}
