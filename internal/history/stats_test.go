package history

import (
	"path/filepath"
	"testing"
	"time"
)

func TestRepositoryStatsAndOccurrences(t *testing.T) {
	store, err := Open(filepath.Join(t.TempDir(), "history.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()

	now := time.Date(2025, 5, 1, 12, 0, 0, 0, time.UTC)
	for i, entry := range []struct {
		dir     string
		daysAgo int
		result  int32
	}{
		{"/repo/sub", 29, 0}, {"/physical/sub", 1, 7}, {"/repo", 0, -1},
		{"/repo-old", 0, 9}, {"/elsewhere", 0, 0},
	} {
		session := string(rune('a' + i))
		start := now.AddDate(0, 0, -entry.daysAgo).UnixMilli()
		if err := store.RecordStart("git status", entry.dir, "fish", session, start); err != nil {
			t.Fatal(err)
		}
		if entry.result != -1 {
			if err := store.RecordFinish(session, start+int64(10*i+5), entry.result); err != nil {
				t.Fatal(err)
			}
		}
	}
	ctx := SearchContext{Cwd: "/repo", Repo: "/repo", PhysicalRepo: "/physical", SessionID: "b"}
	found, err := store.Candidates(ctx, Repository)
	if err != nil || len(found) != 1 || found[0].UseCount != 3 || found[0].CwdUses != 1 {
		t.Fatalf("repository candidates: %+v, %v", found, err)
	}
	rows, err := store.Occurrences(found[0].ID, 2, ctx, Repository)
	if err != nil || len(rows) != 2 || rows[0].Cwd != "/repo" || rows[1].Cwd != "/physical/sub" {
		t.Fatalf("repository occurrences: %+v, %v", rows, err)
	}
	stats, err := store.Stats(ctx, Repository, now.UnixMilli())
	if err != nil || stats.Total != 3 || stats.Unique != 1 || stats.Completed != 2 || stats.Failed != 1 ||
		stats.Average == nil || *stats.Average != 10 ||
		stats.Activity[0] != 1 || stats.Activity[28] != 1 || stats.Activity[29] != 1 ||
		len(stats.Top) != 1 || len(stats.Failures) != 1 || len(stats.Slow) != 1 {
		t.Fatalf("repository statistics: %+v, %v", stats, err)
	}
	if scoped, err := store.Stats(ctx, Session, now.UnixMilli()); err != nil || scoped.Total != 1 || scoped.Failed != 1 {
		t.Fatalf("session statistics: %+v, %v", scoped, err)
	}
}
