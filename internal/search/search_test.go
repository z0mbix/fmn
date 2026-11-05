package search

import (
	"path/filepath"
	"slices"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/z0mbix/fmn/internal/history"
)

func TestCatppuccinListingKeepsCatalogueOrderAndHighlightsFamily(t *testing.T) {
	matches := matchThemes("CATPPUCCIN")
	want := []string{"catppuccin-mocha", "catppuccin-macchiato", "catppuccin-frappe", "catppuccin-latte"}
	var names []string
	for _, match := range matches {
		names = append(names, themes[match.index].name)
		if len(match.indices) != len("catppuccin") {
			t.Fatalf("wrong highlights for %s: %v", themes[match.index].name, match.indices)
		}
		for i, offset := range match.indices {
			if i != offset {
				t.Fatalf("matched outside the family prefix: %v", match.indices)
			}
		}
	}
	if !slices.Equal(names, want) {
		t.Fatalf("got %v, want %v", names, want)
	}
	aliases := map[string]string{"rpd": "rose-pine-dawn", "no-color": "no-color", "EVErFOREST-LIGHT-SOFT": "everforest-light-soft"}
	for query, want := range aliases {
		matches := matchThemes(query)
		if len(matches) == 0 || themes[matches[0].index].name != want {
			t.Errorf("theme query %q did not put %s first", query, want)
		}
	}
}

func TestPatternSupportsRustQueries(t *testing.T) {
	cases := []struct {
		query, command string
		match          bool
	}{
		{"gst", "git status", true}, {"status git", "git status", true},
		{"^git !push", "git status", true}, {"^git !push", "git push", false},
		{"^git", "env git", false}, {"'git", "get it", false},
		{"ls$", "cat -ls", true}, {"^ls$", "ls -la", false},
		{`git\ status`, "git status", true}, {`^git\ status$`, "git status", true},
		{"Git", "git status", false}, {"cafe", "echo café", true},
		{`\!cmd`, "echo !cmd", true},
	}
	for _, tc := range cases {
		_, _, got := matchPattern(parsePattern(tc.query), tc.command)
		if got != tc.match {
			t.Errorf("matching %q against %q: got %v, want %v", tc.query, tc.command, got, tc.match)
		}
	}
}

func TestPickerScopesAndDeletesOnlyConfirmedOccurrence(t *testing.T) {
	store, err := history.Open(filepath.Join(t.TempDir(), "history.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	for i, cwd := range []string{"/elsewhere", "/work", "/work"} {
		if err := store.RecordStart("git status", cwd, "fish", "s", int64(i+1)); err != nil {
			t.Fatal(err)
		}
	}
	m, err := newModel(store, Options{Cwd: "/work", Preferences: Preferences{ShowInspector: true}})
	if err != nil {
		t.Fatal(err)
	}
	key := func(msg tea.Msg) {
		next, _ := m.Update(msg)
		m = next.(model)
		if m.err != nil {
			t.Fatal(m.err)
		}
	}
	key(tea.KeyPressMsg{Code: tea.KeyTab})
	if m.scope != history.Directory || len(m.preview) != 2 {
		t.Fatalf("unexpected scoped preview: %s, %+v", m.scope, m.preview)
	}
	key(tea.KeyPressMsg{Code: 'd', Mod: tea.ModCtrl})
	if m.screen != detailsScreen || len(m.details) != 2 {
		t.Fatalf("unexpected details: %v, %+v", m.screen, m.details)
	}
	key(tea.KeyPressMsg{Code: 'd', Text: "d"})
	key(tea.PasteMsg{Content: "forget"})
	key(tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.screen != confirmScreen {
		t.Fatal("accepted the wrong confirmation word")
	}
	key(tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.screen != detailsScreen {
		t.Fatal("cancel should return to occurrences")
	}
	key(tea.KeyPressMsg{Code: 'd', Text: "d"})
	key(tea.PasteMsg{Content: "delete"})
	key(tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.screen != mainScreen || len(m.preview) != 1 {
		t.Fatalf("deletion did not reload current scope: %v, %+v", m.screen, m.preview)
	}
	entries, err := store.Entries(10)
	if err != nil || len(entries) != 2 || entries[0].ID != 2 || entries[1].ID != 1 {
		t.Fatalf("deleted the wrong occurrences: %+v, %v", entries, err)
	}
	key(tea.KeyPressMsg{Code: 'g', Mod: tea.ModCtrl})
	if m.screen != statsScreen || m.stats.Total != 1 {
		t.Fatalf("wrong scoped statistics: %v, %+v", m.screen, m.stats)
	}
}

func TestFocusRendersThemesAtWideAndNarrowSizes(t *testing.T) {
	store, err := history.Open(filepath.Join(t.TempDir(), "history.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	if err := store.RecordStart("git status", "/work", "fish", "s", 1); err != nil {
		t.Fatal(err)
	}
	m, err := newModel(store, Options{Cwd: "/work", Preferences: Preferences{ShowInspector: true}})
	if err != nil {
		t.Fatal(err)
	}
	for i := range themes {
		m.theme = i
		for _, size := range [][2]int{{122, 34}, {86, 34}, {30, 10}, {10, 4}} {
			m.width, m.height = size[0], size[1]
			view := m.View()
			if !strings.Contains(ansi.Strip(view.Content), "fmn") {
				t.Fatalf("%s at %v lost title", themes[i].label, size)
			}
			m.openThemes(mainScreen)
			view = m.View()
			if i == themeIndex("no-color") && (strings.Contains(view.Content, "38;") || strings.Contains(view.Content, "48;")) {
				t.Fatalf("no-colour rendered colours at %v", size)
			}
			m.screen = mainScreen
		}
	}
}

func FuzzPattern(f *testing.F) {
	f.Add("echo !secret", "echo café")
	f.Add(`^git\ status$`, "git status")
	f.Add("'x", "👋")
	f.Fuzz(func(t *testing.T, query, command string) {
		_, offsets, matched := matchPattern(parsePattern(query), command)
		if !matched {
			return
		}
		for i, offset := range offsets {
			if offset < 0 || offset >= len(command) || i > 0 && offsets[i-1] >= offset {
				t.Fatalf("invalid highlight offsets %v for %q", offsets, command)
			}
		}
	})
}

func TestThemePreviewMouseSelectionAndSettingsShortcutChanges(t *testing.T) {
	store, err := history.Open(filepath.Join(t.TempDir(), "history.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	if err := store.RecordStart("git status", "/work", "fish", "s", 1000); err != nil {
		t.Fatal(err)
	}
	var changes []Change
	m, err := newModel(store, Options{
		Cwd: "/work", Widget: true,
		Preferences: Preferences{Theme: "mint", ShowInspector: true},
		Save: func(change Change) error {
			changes = append(changes, change)
			return nil
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	send := func(msg tea.Msg) {
		t.Helper()
		next, _ := m.Update(msg)
		m = next.(model)
		if m.err != nil {
			t.Fatal(m.err)
		}
	}
	assertView := func(text string) {
		t.Helper()
		if !strings.Contains(ansi.Strip(m.View().Content), text) {
			t.Fatalf("screen %v did not show %q", m.screen, text)
		}
	}
	control := func(key rune) { send(tea.KeyPressMsg{Code: key, Mod: tea.ModCtrl}) }
	press := func(key rune) { send(tea.KeyPressMsg{Code: key}) }

	control('s')
	assertView("Run selection immediately")
	press(tea.KeyEnter)
	send(tea.PasteMsg{Content: "catppuccin"})
	if m.screen != themeScreen || themes[m.theme].name != "catppuccin-mocha" || len(changes) != 0 {
		t.Fatalf("preview changed settings or did not apply visually: %v, %+v", m.screen, changes)
	}
	press(tea.KeyEscape)
	if m.screen != settingsScreen || themes[m.theme].name != "mint" || len(changes) != 0 {
		t.Fatalf("preview cancellation did not restore settings: %v, %+v", m.screen, changes)
	}
	press(tea.KeyEnter)
	send(tea.PasteMsg{Content: "catppuccin"})
	rows := m.themeRows()
	send(tea.MouseClickMsg{X: rows.Min.X, Y: rows.Min.Y + 2, Button: tea.MouseLeft})
	if m.screen != settingsScreen || themes[m.theme].name != "catppuccin-frappe" ||
		len(changes) != 1 || changes[0] != (Change{"theme", "catppuccin-frappe"}) {
		t.Fatalf("click did not save the filtered theme: %v, %+v", m.screen, changes)
	}

	for range 3 {
		press(tea.KeyDown)
	}
	press(tea.KeyEnter)
	if !m.execute || changes[len(changes)-1] != (Change{"execute_on_select", true}) {
		t.Fatal("execution preference did not affect this widget")
	}
	press(tea.KeyDown)
	press(tea.KeyEnter)
	press(tea.KeyEnter)
	control('t')
	assertView("already bound")
	if !m.captureKey || !strings.Contains(m.notice, "already bound") {
		t.Fatal("shortcut editor accepted an existing binding")
	}
	control('a')
	assertView("Ctrl-A")
	if m.captureKey || changes[len(changes)-1] != (Change{"keybindings.inspector", "ctrl-a"}) {
		t.Fatal("shortcut was not saved")
	}
	press(tea.KeyEscape)
	press(tea.KeyEscape)
	control('a')
	if m.screen != mainScreen || m.showInspector || changes[len(changes)-1] != (Change{"show_inspector", false}) {
		t.Fatal("rebound inspector shortcut did not apply or save")
	}
}

func TestActionsHelpAndStatisticsDisplayLiveHistory(t *testing.T) {
	store, err := history.Open(filepath.Join(t.TempDir(), "history.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	m, err := newModel(store, Options{Cwd: "/work", Preferences: Preferences{ShowInspector: true}})
	if err != nil {
		t.Fatal(err)
	}
	for i, command := range []string{"git status", "git status", "go test"} {
		at := m.now - int64(i*1000+1000)
		if err := store.RecordStart(command, "/work", "fish", "s", at); err != nil {
			t.Fatal(err)
		}
		if err := store.RecordFinish("s", at+200, int32(i%2)); err != nil {
			t.Fatal(err)
		}
	}
	if err := m.reload(); err != nil {
		t.Fatal(err)
	}
	update := func(msg tea.Msg) {
		next, _ := m.Update(msg)
		m = next.(model)
		if m.err != nil {
			t.Fatal(m.err)
		}
	}
	key := func(code rune) { update(tea.KeyPressMsg{Code: code}) }
	control := func(code rune) { update(tea.KeyPressMsg{Code: code, Mod: tea.ModCtrl}) }
	see := func(want string) {
		t.Helper()
		if !strings.Contains(ansi.Strip(m.View().Content), want) {
			t.Errorf("screen %v did not show %q", m.screen, want)
		}
	}

	key('?')
	see("Toggle the inspector")
	key(tea.KeyEscape)
	control('g')
	see("3 executions")
	see("33.3% failed")
	see("200ms")
	key(tea.KeyDown)
	key(tea.KeyTab)
	see("This directory")
	key(tea.KeyEscape)
	control('x')
	see("Forget this command")
	key('i')
	see("Occurrences")
	see("200ms")
	see("exit 1")
	control('x')
	key('f')
	see("Type forget")
	update(tea.PasteMsg{Content: "forget"})
	key(tea.KeyEnter)
	rows, err := store.Entries(10)
	if err != nil || len(rows) != 1 || rows[0].Command != "go test" {
		t.Fatalf("forget did not remove all selected occurrences: %+v, %v", rows, err)
	}
}

func TestInputEditsWholeGraphemes(t *testing.T) {
	input := newInput("", "")
	input.SetValue("a👩‍💻é")
	input.CursorEnd()
	input, _ = updateInput(input, tea.KeyPressMsg{Code: tea.KeyLeft})
	input, _ = updateInput(input, tea.KeyPressMsg{Code: tea.KeyBackspace})
	if input.Value() != "aé" {
		t.Fatalf("backspace split a grapheme: %q", input.Value())
	}
	input, _ = updateInput(input, tea.KeyPressMsg{Code: tea.KeyDelete})
	if input.Value() != "a" {
		t.Fatalf("delete did not remove the next grapheme: %q", input.Value())
	}
}
