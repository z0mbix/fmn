package history

import (
	"fmt"
	"path/filepath"
	"sync"
	"testing"
)

func TestConcurrentCaptureAndDuplicateImports(t *testing.T) {
	path := filepath.Join(t.TempDir(), "history.db")
	const writers = 12
	start := make(chan struct{})
	errors := make(chan error, writers)
	var group sync.WaitGroup
	for writer := range writers {
		group.Go(func() {
			<-start
			store, err := Open(path)
			if err != nil {
				errors <- err
				return
			}
			defer store.Close()
			session := fmt.Sprint(writer)
			at := int64(writer * 100)
			if err := store.RecordStart("echo concurrent", "/work", "fish", session, at); err != nil {
				errors <- err
				return
			}
			if err := store.RecordFinish(session, at+5, 0); err != nil {
				errors <- err
				return
			}
			_, err = store.Import(ImportRecord{Command: "echo imported", Key: "same-import"}, "fish", at)
			if err != nil {
				errors <- err
			}
		})
	}
	close(start)
	group.Wait()
	close(errors)
	for err := range errors {
		t.Error(err)
	}
	if t.Failed() {
		return
	}

	store, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	rows, err := store.Entries(writers + 2)
	if err != nil || len(rows) != writers+1 {
		t.Fatalf("expected %d live occurrences and one import, got %d: %v", writers, len(rows), err)
	}
	for _, row := range rows {
		if row.Source == "live" && (row.DurationMS == nil || *row.DurationMS != 5) {
			t.Errorf("unmatched finish for session %s: %+v", row.SessionID, row)
		}
	}
	candidates, err := store.Candidates(SearchContext{Cwd: "/work"}, Directory)
	if err != nil || len(candidates) != 1 || candidates[0].UseCount != writers {
		t.Fatalf("incorrect directory aggregate: %+v, %v", candidates, err)
	}
}
