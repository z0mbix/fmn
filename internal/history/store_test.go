package history

import (
	"database/sql"
	"path/filepath"
	"testing"
)

func TestUpgradeLegacyHistory(t *testing.T) {
	path := filepath.Join(t.TempDir(), "history.db")
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	schema, err := migrations.ReadFile("schema/1.sql")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec(string(schema)); err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec(`
		INSERT INTO commands VALUES (1, 'git status', 10, 10, 1);
		INSERT INTO entries (command_id, started_at, cwd, shell, session_id, source)
		VALUES (1, 10, '/repo', 'zsh', 'old', 'import');`); err != nil {
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}

	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	if err := store.RecordStart("git status", "/repo", "fish", "new", 20); err != nil {
		t.Fatal(err)
	}
	if err := store.RecordFinish("new", 33, 7); err != nil {
		t.Fatal(err)
	}
	entries, err := store.Entries(10)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 || entries[1].Source != "import" || entries[0].CommandID != entries[1].CommandID {
		t.Fatalf("expected live and imported occurrences of the same command, got %+v", entries)
	}
	recent := entries[0]
	if recent.DurationMS == nil || *recent.DurationMS != 13 || recent.ExitCode == nil || *recent.ExitCode != 7 {
		t.Fatalf("unexpected finish metadata: %+v", recent)
	}
	context := SearchContext{Cwd: "/repo", SessionID: "new"}
	for scope, uses := range map[Scope]int64{Global: 2, Directory: 2, Session: 1} {
		results, err := store.Candidates(context, scope)
		if err != nil {
			t.Fatal(err)
		}
		if len(results) != 1 || results[0].UseCount != uses || results[0].CwdUses != uses {
			t.Errorf("%s search should find %d uses of one command, got %+v", scope, uses, results)
		}
	}
}

func TestFutureSchemaIsRejectedWithoutMigration(t *testing.T) {
	path := filepath.Join(t.TempDir(), "history.db")
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if _, err := db.Exec(`
		CREATE TABLE schema_migrations (version INTEGER);
		INSERT INTO schema_migrations VALUES (99);
	`); err != nil {
		t.Fatal(err)
	}
	if store, err := Open(path); err == nil {
		store.Close()
		t.Fatal("accepted an unsupported future schema")
	}
	var journal string
	if err := db.QueryRow("PRAGMA journal_mode").Scan(&journal); err != nil || journal != "delete" {
		t.Fatalf("opening a future schema changed its journal mode: %q, %v", journal, err)
	}
}
