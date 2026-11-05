package search

import (
	"fmt"
	"math"
	"slices"
	"strings"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/z0mbix/fmn/internal/history"
)

var settingLabels = []string{"Colour theme", "Show inspector", "Default scope", "Run selection immediately", "Keybindings"}

func (m *model) changeSetting(backwards bool) {
	switch m.settingCursor {
	case 0:
		m.openThemes(settingsScreen)
	case 1:
		m.toggleInspector()
	case 2:
		index := slices.Index(scopeOrder[:], m.prefs.DefaultScope)
		step := 1
		if backwards {
			step = -1
		}
		value := scopeOrder[(max(0, index)+step+len(scopeOrder))%len(scopeOrder)]
		if m.persist(Change{"default_scope", string(value)}) {
			m.prefs.DefaultScope = value
			m.scope = m.context.Effective(value)
			m.settingsBack = mainScreen
			m.err = m.reload()
		}
	case 3:
		value := !m.prefs.ExecuteOnSelect
		if m.persist(Change{"execute_on_select", value}) {
			m.prefs.ExecuteOnSelect, m.execute = value, m.widget && value
		}
	case 4:
		m.captureKey = false
		m.openMenu(bindingsScreen)
	}
}

func (m model) settingsRows() uv.Rectangle {
	r := m.popup(76, 19)
	return uv.Rect(r.Min.X+2, r.Min.Y+4, max(0, r.Dx()-4), min(len(settingLabels), max(0, r.Dy()-8)))
}

func (m model) drawSettings(c canvas, p palette) {
	r, p := m.dialog(c, p, 76, 19, "Settings")
	c.put(line(r, 1), p.style(p.muted).Render("Changes save automatically"))
	inspector, execute := "Off", "Off (insert into prompt)"
	if m.showInspector {
		inspector = "On"
	}
	if m.prefs.ExecuteOnSelect {
		execute = "On"
	}
	values := []string{themes[m.theme].label, inspector, m.prefs.DefaultScope.Label(), execute, "Edit shortcuts…"}
	rows := m.settingsRows()
	first := visibleStart(m.settingCursor, len(settingLabels), rows.Dy())
	for i := first; i < min(first+rows.Dy(), len(settingLabels)); i++ {
		m.drawMenuItem(c, line(rows, i-first), p, i == m.settingCursor, settingLabels[i], values[i])
	}
	hint := []string{"Open the searchable theme picker", "Show command metadata beside the results",
		"Applies now and when you next open fmn", "Enter runs commands when using shell integration",
		"Reassign shortcuts; each change saves automatically"}[m.settingCursor]
	if m.settingCursor == 2 && m.context.Effective(m.prefs.DefaultScope) != m.prefs.DefaultScope {
		hint = "Unavailable here; uses global scope until available"
	}
	if m.notice != "" {
		hint = m.notice
	}
	c.put(line(r, r.Dy()-3), p.style(p.muted).Render(displayText(hint)))
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render("↑↓ select · ↵ / ←→ change · esc close"))
}

func (m model) bindingRows() uv.Rectangle {
	r := m.popup(76, 22)
	return uv.Rect(r.Min.X+2, r.Min.Y+4, max(0, r.Dx()-4), min(int(actionCount), max(0, r.Dy()-8)))
}

func (m model) updateBindings(msg tea.Msg) (tea.Model, tea.Cmd) {
	if !m.captureKey {
		switch msg := msg.(type) {
		case tea.MouseWheelMsg:
			if inRows(msg, m.bindingRows()) {
				m.menuCursor = bounded(m.menuCursor+wheelDelta(msg), int(actionCount))
			}
			return m, nil
		case tea.MouseClickMsg:
			if index := clickedRow(msg, m.bindingRows(), m.menuCursor, int(actionCount)); index >= 0 {
				m.menuCursor, m.captureKey, m.notice = index, true, ""
			}
			return m, nil
		}
	}
	keyMsg, ok := msg.(tea.KeyPressMsg)
	if !ok {
		return m, nil
	}
	key := keyMsg.String()
	action := Action(m.menuCursor)
	if m.captureKey {
		if key == "esc" {
			m.captureKey, m.notice = false, ""
			return m, nil
		}
		parsed, err := parseBinding(key)
		if err != nil {
			m.notice = err.Error()
			return m, nil
		}
		if m.setBinding(action, parsed) {
			m.captureKey = false
		}
		return m, nil
	}
	switch key {
	case "esc":
		m.screen, m.notice = settingsScreen, ""
	case "enter":
		m.captureKey, m.notice = true, "Press the new shortcut · esc cancel"
	case "backspace", "delete":
		m.setBinding(action, DefaultBindings()[action])
	case "r":
		defaults := DefaultBindings()
		if m.persist(Change{"keybindings", defaults}) {
			m.bindings, m.prefs.Bindings = defaults, defaults
		}
	default:
		m.menuCursor = nav(m.menuCursor, int(actionCount), int(actionCount), key)
	}
	return m, nil
}

func (m *model) setBinding(action Action, key string) bool {
	bindings, change, err := m.bindings.change(action, key)
	if err != nil {
		m.notice = err.Error()
		return false
	}
	if !m.persist(change) {
		return false
	}
	m.bindings, m.prefs.Bindings = bindings, bindings
	return true
}

func (m model) drawBindings(c canvas, p palette) {
	r, p := m.dialog(c, p, 76, 22, "Keybindings")
	c.put(line(r, 1), p.style(p.muted).Render("Select an action, then press Enter to rebind"))
	rows := m.bindingRows()
	n := rows.Dy()
	first := visibleStart(m.menuCursor, int(actionCount), n)
	for i := first; i < min(first+n, int(actionCount)); i++ {
		value := m.bindings.label(Action(i))
		if strings.HasPrefix(m.bindings[i], "ctrl+") {
			value = "Ctrl-" + value[1:]
		}
		if m.captureKey && i == m.menuCursor {
			value = "Press a key…"
		}
		m.drawMenuItem(c, line(rows, i-first), p, i == m.menuCursor, actionLabels[i], value)
	}
	hint := "Tab and Ctrl-I share a terminal key code"
	if m.notice != "" {
		hint = m.notice
	}
	c.put(line(r, r.Dy()-3), p.style(p.muted).Render(displayText(hint)))
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render("↑↓ select · ↵ edit · ⌫ reset · r reset all · esc"))
}

func (m model) statsHeight() int {
	return max(1, m.popup(82, max(1, m.height-4)).Dy()-6)
}

func (m model) statsScrollLimit() int {
	return max(0, len(m.statLines(palette{}))-m.statsHeight())
}

func averageDuration(average *float64) string {
	if average == nil {
		return duration(nil)
	}
	ms := int64(*average)
	return duration(&ms)
}

func (m model) statLines(p palette) []string {
	accent, muted, text := p.style(p.accent).Bold(true), p.style(p.muted), p.style(p.text)
	stats := m.stats
	rate := "—"
	if stats.Completed > 0 {
		rate = fmt.Sprintf("%.1f%%", 100*float64(stats.Failed)/float64(stats.Completed))
	}
	lines := []string{
		accent.Render(fmt.Sprintf("%d executions", stats.Total)) + text.Render(fmt.Sprintf("  ·  %d unique commands", stats.Unique)),
		p.style(p.failure).Render(rate+" failed") + muted.Render(fmt.Sprintf("  ·  %d known / %d unknown outcomes", stats.Completed, stats.Total-stats.Completed)),
		muted.Render("Average duration  ") + text.Render(averageDuration(stats.Average)), "",
		accent.Render("ACTIVITY · LAST 30 UTC DAYS"),
	}
	peak := slices.Max(stats.Activity[:])
	bars := []rune("▁▂▃▄▅▆▇█")
	sparkline := ""
	for _, count := range stats.Activity {
		if count == 0 {
			sparkline += muted.Render("·")
		} else {
			sparkline += accent.Render(string(bars[int(math.Round(7*float64(count)/float64(peak)))]))
		}
	}
	lines = append(lines, sparkline, muted.Render(fmt.Sprintf("29 days ago → today  ·  peak %d/day", peak)), "")
	for i, group := range []struct {
		label string
		items []history.CommandStats
	}{
		{"MOST USED", stats.Top}, {"FAILED COMMANDS", stats.Failures}, {"SLOWEST · AVERAGE DURATION", stats.Slow},
	} {
		lines = append(lines, accent.Render(group.label))
		if len(group.items) == 0 {
			lines = append(lines, muted.Render("No recorded commands"))
		}
		for _, item := range group.items {
			metric := fmt.Sprintf("%d×", item.Count)
			if i == 1 {
				metric = fmt.Sprintf("%d failed", item.Failures)
			}
			if i == 2 {
				metric = averageDuration(item.Average)
			}
			lines = append(lines, muted.Render(fmt.Sprintf("%-12s", metric))+text.Render(displayText(item.Command)))
		}
		lines = append(lines, "")
	}
	return lines
}

func (m model) drawStats(c canvas, p palette) {
	r, p := m.dialog(c, p, 82, max(1, m.height-4), "History statistics")
	c.put(line(r, 0), p.style(p.accent).Render("◉ "+m.scope.Label()))
	lines := m.statLines(p)
	first := min(m.menuCursor, m.statsScrollLimit())
	for i := first; i < min(first+m.statsHeight(), len(lines)); i++ {
		c.put(line(r, 2+i-first), lines[i])
	}
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render("↑↓ scroll  "+m.bindings.label(ScopeCycle)+" scope  r refresh  esc back"))
}
