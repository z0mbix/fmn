package search

import (
	"fmt"
	"strings"

	"github.com/z0mbix/fmn/internal/history"
)

type Action int

const (
	Inspector Action = iota
	Themes
	Details
	Statistics
	Settings
	ScopeCycle
	Actions
	Help
	actionCount
)

var actionNames = [...]string{
	"inspector", "themes", "details", "statistics", "settings", "scope", "actions", "help",
}
var actionLabels = [...]string{
	"Toggle inspector", "Open themes", "Inspect occurrences", "History statistics",
	"Open settings", "Cycle scope", "Open actions", "Open help",
}

type Bindings [actionCount]string

func DefaultBindings() Bindings {
	return Bindings{"ctrl+o", "ctrl+t", "ctrl+d", "ctrl+g", "ctrl+s", "tab", "ctrl+x", "?"}
}

func ParseBindings(values map[string]string) (Bindings, error) {
	bindings := DefaultBindings()
	for name, value := range values {
		found := false
		for i, key := range actionNames {
			if name == key {
				binding, err := parseBinding(value)
				if err != nil {
					return bindings, fmt.Errorf("%s: %w", name, err)
				}
				bindings[i], found = binding, true
				break
			}
		}
		if !found {
			return bindings, fmt.Errorf("unknown keybinding %q", name)
		}
	}
	return bindings, bindings.validate()
}

func parseBinding(value string) (string, error) {
	key := strings.ToLower(value)
	key = strings.ReplaceAll(key, "-", "+")
	if key == "ctrl+i" {
		key = "tab"
	}
	if key == "tab" || key == "?" {
		return key, nil
	}
	if len(key) == 6 && strings.HasPrefix(key, "ctrl+") && key[5] >= 'a' && key[5] <= 'z' {
		if strings.ContainsRune("chjmnpu", rune(key[5])) {
			return "", fmt.Errorf("%s is reserved", keyLabel(key))
		}
		return key, nil
	}
	return "", fmt.Errorf("use Ctrl plus a letter, Tab, or ? for Help")
}

func (b Bindings) validate() error {
	for i, key := range b {
		if key == "?" && Action(i) != Help {
			return fmt.Errorf("? can only open Help")
		}
		for j := 0; j < i; j++ {
			if b[j] == key {
				return fmt.Errorf("%s is already bound to %s", keyLabel(key), actionLabels[j])
			}
		}
	}
	return nil
}

func keyLabel(key string) string {
	if key == "tab" {
		return "Tab"
	}
	if strings.HasPrefix(key, "ctrl+") {
		return "^" + strings.ToUpper(key[5:])
	}
	return key
}

func (b Bindings) label(action Action) string { return keyLabel(b[action]) }

type Preferences struct {
	Theme           string
	ShowInspector   bool
	ExecuteOnSelect bool
	DefaultScope    history.Scope
	Bindings        Bindings
}

// Change describes one config edit.
type Change struct {
	Key   string
	Value any
}

func (b Bindings) change(action Action, key string) (Bindings, Change, error) {
	b[action] = key
	value := strings.ReplaceAll(key, "+", "-")
	return b, Change{Key: "keybindings." + actionNames[action], Value: value}, b.validate()
}

func (b Bindings) ConfigValues() map[string]string {
	values := make(map[string]string, len(b))
	for action, key := range b {
		values[actionNames[action]] = strings.ReplaceAll(key, "+", "-")
	}
	return values
}
