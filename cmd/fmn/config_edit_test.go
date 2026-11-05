package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/z0mbix/fmn/internal/search"
)

func TestPreferenceEditsKeepTOMLAndDotfileSymlink(t *testing.T) {
	for _, original := range []string{
		"# keep\n[ui]\ntheme = 'nord' # appearance\n[ui.keybindings]\ninspector = 'ctrl-o' # shortcut\nstatistics = 'ctrl-b'\n",
		"# keep\nui = { theme = 'nord', keybindings = { inspector = 'ctrl-o', statistics = 'ctrl-b' } }\n",
		"# keep\nui.theme = 'nord'\nui.keybindings.inspector = 'ctrl-o' # shortcut\nui.keybindings.statistics = 'ctrl-b'\n",
	} {
		root := t.TempDir()
		target, link := filepath.Join(root, "target.toml"), filepath.Join(root, "config.toml")
		if err := os.WriteFile(target, []byte(original), 0o640); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(target, link); err != nil {
			t.Fatal(err)
		}
		if err := savePreference(link, search.Change{Key: "keybindings.inspector", Value: "ctrl-a"}); err != nil {
			t.Fatal(err)
		}
		contents, err := os.ReadFile(target)
		if err != nil {
			t.Fatal(err)
		}
		if want := strings.Replace(original, "'ctrl-o'", `"ctrl-a"`, 1); string(contents) != want {
			t.Fatalf("edit changed surrounding TOML\n%s\nwant\n%s", contents, want)
		}
		if err := savePreference(link, search.Change{Key: "execute_on_select", Value: true}); err != nil {
			t.Fatal(err)
		}
		if err := savePreference(link, search.Change{Key: "keybindings", Value: search.DefaultBindings()}); err != nil {
			t.Fatal(err)
		}
		cfg, err := loadConfig(link)
		if err != nil || !cfg.UI.ExecuteOnSelect || cfg.UI.Theme != "nord" {
			t.Fatalf("saved config: %+v, %v", cfg, err)
		}
		got, err := search.ParseBindings(cfg.UI.Keybindings)
		if err != nil || got != search.DefaultBindings() {
			t.Fatalf("reset bindings: %+v, %v", got, err)
		}
		info, err := os.Lstat(link)
		if err != nil || info.Mode()&os.ModeSymlink == 0 {
			t.Fatalf("config symlink changed: %v, %v", info, err)
		}
		info, err = os.Stat(target)
		if err != nil || info.Mode().Perm() != 0o640 {
			t.Fatalf("config permissions changed: %v, %v", info, err)
		}
	}
}

func TestPreferenceConflictDoesNotRewriteConfig(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	original := "[ui.keybindings]\nthemes = 'ctrl-a'\n"
	if err := os.WriteFile(path, []byte(original), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := savePreference(path, search.Change{Key: "keybindings.inspector", Value: "ctrl-a"}); err == nil {
		t.Fatal("saved a conflicting shortcut")
	}
	got, err := os.ReadFile(path)
	if err != nil || string(got) != original {
		t.Fatalf("config changed: %q, %v", got, err)
	}
}

func FuzzConfig(f *testing.F) {
	f.Add("[ui]\ntheme = 'nord'\n[history]\nignore_patterns = ['secret *']\n")
	f.Add("ui = { keybindings = { scope = 'tab' } }\n")
	f.Add("[ui.keybindings]\nthemes = 'ctrl-x'\n")
	f.Fuzz(func(t *testing.T, input string) {
		if _, err := parseConfig([]byte(input)); err != nil {
			return
		}
		updated, err := editConfig([]byte(input), []string{"ui", "theme"}, `"rose"`)
		if err != nil {
			t.Fatal(err)
		}
		cfg, err := parseConfig(updated)
		if err != nil || cfg.UI.Theme != "rose" {
			t.Fatalf("valid config became unusable after editing: %s (%v)", updated, err)
		}
	})
}
