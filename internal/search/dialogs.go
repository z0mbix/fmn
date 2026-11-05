package search

import (
	"errors"
	"fmt"
	"time"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/z0mbix/fmn/internal/history"
)

type confirmation struct {
	id    int64
	word  string
	back  screen
	input textinput.Model
}

func (m *model) openActions() {
	m.overlayBack = m.screen
	m.openMenu(actionsScreen)
}

func (m *model) openDetails() {
	if selected := m.selected(); selected != nil {
		m.details, m.err = m.store.Occurrences(selected.ID, -1, m.context, m.scope)
		m.detailCursor = 0
		m.openMenu(detailsScreen)
	}
}

func (m *model) openSettings(back screen) {
	m.screen, m.settingsBack, m.settingCursor, m.notice = settingsScreen, back, 0, ""
}

func (m *model) openStats(back screen) {
	m.openMenu(statsScreen)
	m.statsBack = back
	m.stats, m.err = m.store.Stats(m.context, m.scope, m.now)
}

func (m model) drawOverlay(c canvas, p palette) *tea.Cursor {
	if m.screen != detailsScreen &&
		(m.screen == actionsScreen && m.overlayBack == detailsScreen ||
			m.screen == confirmScreen && m.confirm.back == detailsScreen ||
			m.screen == statsScreen && m.statsBack == detailsScreen ||
			m.screen == helpScreen && m.helpBack == detailsScreen ||
			m.screen == themeScreen && m.themeBack == detailsScreen ||
			(m.screen == settingsScreen || m.screen == bindingsScreen) && m.settingsBack == detailsScreen) {
		c.dim(p)
		m.drawDetails(c, p)
	}
	c.dim(p)
	switch m.screen {
	case themeScreen:
		return m.drawThemes(c, p)
	case detailsScreen:
		m.drawDetails(c, p)
	case actionsScreen:
		m.drawActions(c, p)
	case helpScreen:
		m.drawHelp(c, p)
	case statsScreen:
		m.drawStats(c, p)
	case settingsScreen:
		m.drawSettings(c, p)
	case bindingsScreen:
		m.drawBindings(c, p)
	case confirmScreen:
		return m.drawConfirm(c, p)
	}
	return nil
}

func (m model) updateOverlay(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch m.screen {
	case themeScreen:
		return m.updateTheme(msg)
	case confirmScreen:
		return m.updateConfirm(msg)
	case bindingsScreen:
		return m.updateBindings(msg)
	}

	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		key := msg.String()
		switch m.screen {
		case helpScreen:
			m.screen = m.helpBack
		case detailsScreen:
			switch key {
			case "esc", "left", m.bindings[Details]:
				m.screen = mainScreen
			case "enter":
				if m.detailCursor < len(m.details) {
					return m.accept(m.details[m.detailCursor].Command)
				}
			case "d":
				m.beginMutation("delete", detailsScreen)
			case m.bindings[Actions]:
				m.openActions()
			case m.bindings[Inspector]:
				m.toggleInspector()
			case m.bindings[Themes]:
				m.openThemes(detailsScreen)
			case m.bindings[Statistics]:
				m.openStats(detailsScreen)
			case m.bindings[Settings]:
				m.openSettings(detailsScreen)
			case m.bindings[Help]:
				m.screen, m.helpBack = helpScreen, detailsScreen
			case m.bindings[ScopeCycle], "shift+tab":
				m.scope = m.context.Next(m.scope, key == "shift+tab")
				m.screen, m.err = mainScreen, m.reload()
			default:
				m.detailCursor = nav(m.detailCursor, len(m.details), m.detailRows().Dy(), key)
			}
		case actionsScreen:
			items := m.menuActions()
			switch key {
			case "esc":
				m.screen = m.overlayBack
			case "enter":
				return m.runAction(items[m.menuCursor].key)
			default:
				for _, item := range items {
					if key == item.key {
						return m.runAction(key)
					}
				}
				m.menuCursor = nav(m.menuCursor, len(items), m.actionRows().Dy(), key)
			}
		case statsScreen:
			switch key {
			case "esc", m.bindings[Statistics]:
				m.screen = m.statsBack
			case m.bindings[ScopeCycle], "shift+tab", "r":
				if key != "r" {
					m.scope = m.context.Next(m.scope, key == "shift+tab")
					m.err = m.reload()
					m.statsBack = mainScreen
				}
				if m.err == nil {
					m.now = time.Now().UnixMilli()
					m.openStats(m.statsBack)
				}
			default:
				m.menuCursor = nav(m.menuCursor, m.statsScrollLimit()+1, m.statsHeight(), key)
			}
		case settingsScreen:
			switch key {
			case "esc", m.bindings[Settings]:
				m.screen = m.settingsBack
			case "enter", "left", "right", "space", " ":
				m.changeSetting(key == "left")
			default:
				m.settingCursor = nav(m.settingCursor, len(settingLabels), len(settingLabels), key)
			}
		}
	case tea.MouseClickMsg:
		switch m.screen {
		case detailsScreen:
			if index := m.clickedOccurrence(msg); index >= 0 {
				m.detailCursor = index
			}
		case actionsScreen:
			items := m.menuActions()
			if index := clickedRow(msg, m.actionRows(), m.menuCursor, len(items)); index >= 0 {
				return m.runAction(items[index].key)
			}
		case settingsScreen:
			if index := clickedRow(msg, m.settingsRows(), m.settingCursor, len(settingLabels)); index >= 0 {
				m.settingCursor = index
				m.changeSetting(false)
			}
		}
	case tea.MouseWheelMsg:
		delta := wheelDelta(msg)
		switch m.screen {
		case detailsScreen:
			if inRows(msg, m.detailRows()) {
				m.detailCursor = bounded(m.detailCursor+delta, len(m.details))
			}
		case actionsScreen:
			m.menuCursor = bounded(m.menuCursor+delta, len(m.menuActions()))
		case statsScreen:
			m.menuCursor = bounded(m.menuCursor+3*delta, m.statsScrollLimit()+1)
		case settingsScreen:
			if inRows(msg, m.settingsRows()) {
				m.settingCursor = bounded(m.settingCursor+delta, len(settingLabels))
			}
		}
	}
	return m.checked()
}

func bounded(selected, length int) int { return min(max(0, selected), max(0, length-1)) }

func nav(selected, length, page int, key string) int {
	switch key {
	case "up", "ctrl+p":
		selected--
	case "down", "ctrl+n":
		selected++
	case "pgup":
		selected -= max(1, page)
	case "pgdown":
		selected += max(1, page)
	case "home":
		selected = 0
	case "end":
		selected = length - 1
	}
	return bounded(selected, length)
}

type menuAction struct{ key, label string }

func (m model) menuActions() []menuAction {
	var items []menuAction
	if m.selected() != nil {
		items = append(items, menuAction{"enter", m.acceptLabel() + " selected command"}, menuAction{"i", "Inspect occurrences"})
	}
	items = append(items, menuAction{"t", "Choose a theme"}, menuAction{"g", "History statistics"}, menuAction{"s", "Open settings"})
	if m.overlayBack == detailsScreen && m.detailCursor < len(m.details) {
		items = append(items, menuAction{"d", "Delete this occurrence"})
	}
	if m.selected() != nil {
		items = append(items, menuAction{"f", "Forget this command everywhere"})
	}
	return items
}

func (m model) runAction(key string) (tea.Model, tea.Cmd) {
	switch key {
	case "enter":
		if item := m.selected(); item != nil {
			return m.accept(item.Command)
		}
	case "i":
		m.openDetails()
	case "t":
		m.openThemes(m.overlayBack)
	case "g":
		m.openStats(m.overlayBack)
	case "s":
		m.openSettings(m.overlayBack)
	case "d":
		if m.overlayBack == detailsScreen {
			m.beginMutation("delete", detailsScreen)
		}
	case "f":
		m.beginMutation("forget", m.overlayBack)
	}
	return m.checked()
}

func (m *model) beginMutation(word string, back screen) {
	var id int64
	if word == "delete" {
		if m.detailCursor >= len(m.details) {
			return
		}
		id = m.details[m.detailCursor].ID
	} else if item := m.selected(); item != nil {
		id = item.ID
	} else {
		return
	}
	m.confirm = confirmation{id, word, back, newInput(" › ", word)}
	m.screen, m.notice = confirmScreen, ""
}

func (m model) updateConfirm(msg tea.Msg) (tea.Model, tea.Cmd) {
	if key, ok := msg.(tea.KeyPressMsg); ok {
		switch key.String() {
		case "esc":
			m.screen, m.notice = m.confirm.back, ""
			return m, nil
		case "enter":
			if m.confirm.input.Value() != m.confirm.word {
				m.notice = "Type " + m.confirm.word + " exactly to confirm"
				return m, nil
			}
			var err error
			if m.confirm.word == "delete" {
				err = m.store.DeleteEntry(m.confirm.id)
			} else {
				err = m.store.ForgetCommand(m.confirm.id)
			}
			if err != nil && !errors.Is(err, history.ErrPurgePending) {
				m.notice = err.Error()
				return m, nil
			}
			m.screen, m.notice = mainScreen, "History updated"
			if err != nil {
				m.notice = err.Error()
			}
			m.details = nil
			m.err = m.reload()
			return m.checked()
		}
	}
	m.confirm.input.SetWidth(max(1, m.popup(64, 10).Dx()-8))
	var cmd tea.Cmd
	m.confirm.input, cmd = updateInput(m.confirm.input, msg)
	return m, cmd
}

func (m model) detailSize() (int, int) { return min(100, max(1, m.width-8)), max(1, m.height-4) }

func (m model) detailRows() uv.Rectangle {
	w, h := m.detailSize()
	r := m.popup(w, h)
	return uv.Rect(r.Min.X+2, r.Min.Y+4, max(0, r.Dx()-4), max(0, r.Dy()-7))
}

func (m model) clickedOccurrence(msg tea.MouseClickMsg) int {
	r := m.detailRows()
	mouse := msg.Mouse()
	if mouse.Button != tea.MouseLeft || !inRows(msg, r) {
		return -1
	}
	page := r.Dy() / 2
	offset := (mouse.Y - r.Min.Y) / 2
	index := visibleStart(m.detailCursor, len(m.details), page) + offset
	if offset >= page || index >= len(m.details) {
		return -1
	}
	return index
}

func (m model) drawDetails(c canvas, p palette) {
	w, h := m.detailSize()
	r, p := m.dialog(c, p, w, h, "Occurrences")
	if item := m.selected(); item != nil {
		c.put(line(r, 1), p.style(p.accent).Render(displayText(item.Command)))
	}
	rows := m.detailRows()
	page := rows.Dy() / 2
	first := visibleStart(m.detailCursor, len(m.details), page)
	for i := first; i < min(first+page, len(m.details)); i++ {
		entry := m.details[i]
		area := uv.Rect(rows.Min.X, rows.Min.Y+(i-first)*2, rows.Dx(), 2)
		current, reverse, mark := p, false, "  "
		if i == m.detailCursor {
			current, reverse, mark = p.selection(), p.selected == nil, "› "
			c.fill(area, current.background, reverse)
		}
		symbol, fg := current.outcome(entry.ExitCode)
		exit := "?"
		if entry.ExitCode != nil {
			exit = fmt.Sprint(*entry.ExitCode)
		}
		style := current.style(current.text).Reverse(reverse)
		c.put(line(area, 0), current.style(current.accent).Reverse(reverse).Render(mark)+
			current.style(fg).Reverse(reverse).Render(symbol+" ")+
			style.Render(fmt.Sprintf("%s · %s · exit %s", m.age(entry.StartedAt), duration(entry.DurationMS), exit)))
		c.put(line(area, 1), current.style(current.muted).Reverse(reverse).Render("    "+shortPath(entry.Cwd)+" · "+displayText(entry.Shell)))
	}
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render("↑↓ select  ↵ "+m.acceptLabel()+"  d delete  "+m.bindings.label(Actions)+" actions  esc back"))
}

func (m model) actionRows() uv.Rectangle {
	r := m.popup(64, len(m.menuActions())+7)
	return uv.Rect(r.Min.X+2, r.Min.Y+4, max(0, r.Dx()-4), max(0, r.Dy()-6))
}

func (m model) drawActions(c canvas, p palette) {
	items := m.menuActions()
	r, p := m.dialog(c, p, 64, len(items)+7, "Command actions")
	c.put(line(r, 0), p.style(p.muted).Render("Choose what to do with the selected command."))
	rows := m.actionRows()
	first := visibleStart(m.menuCursor, len(items), rows.Dy())
	for i := first; i < min(first+rows.Dy(), len(items)); i++ {
		item := items[i]
		m.drawMenuItem(c, line(rows, i-first), p, i == m.menuCursor, item.label, item.key)
	}
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render("↑↓ select  ↵ choose  esc back"))
}

func (m model) drawMenuItem(c canvas, row uv.Rectangle, p palette, selected bool, text, value string) {
	mark, reverse := "  ", false
	if selected {
		mark, reverse, p = "› ", p.selected == nil, p.selection()
		c.fill(row, p.background, reverse)
	}
	c.put(row, p.style(p.text).Reverse(reverse).Render(fit(mark+text, max(1, row.Dx()-len(value)-2))))
	c.right(row, p.style(p.accent).Reverse(reverse), value+" ")
}

func (m model) drawHelp(c canvas, p palette) {
	r, p := m.dialog(c, p, 66, 20, "Help")
	lines := [][2]string{
		{"Type / Backspace", "Refine the query"}, {"↑↓ / ^P ^N", "Move selection"},
		{"↵", m.acceptLabel() + " selection"}, {m.bindings.label(ScopeCycle) + " / Shift-Tab", "Cycle scopes"},
		{m.bindings.label(Inspector), "Toggle the inspector"}, {m.bindings.label(Details), "Inspect occurrences"},
		{m.bindings.label(Themes), "Choose a theme"}, {m.bindings.label(Statistics), "History statistics"},
		{m.bindings.label(Settings), "Settings and keybindings"}, {m.bindings.label(Actions), "Actions"},
		{"Mouse", "Select or scroll"}, {"esc / ^C", "Close or cancel"},
	}
	for i, item := range lines {
		c.put(line(r, i+1), p.style(p.accent).Render(fmt.Sprintf("%-23s", item[0]))+p.style(p.text).Render(item[1]))
	}
}

func (m model) drawConfirm(c canvas, p palette) *tea.Cursor {
	r, p := m.dialog(c, p, 64, 10, "Confirm "+m.confirm.word)
	message := "Permanently remove this occurrence?"
	if m.confirm.word == "forget" {
		message = "Permanently remove this command and all its occurrences?"
	}
	c.put(line(r, 1), p.style(p.failure).Render(message))
	c.put(line(r, 2), p.style(p.muted).Render("Type "+m.confirm.word+" to confirm."))
	cursor := drawInput(c, line(r, 4), m.confirm.input, p, true)
	hint := "↵ confirm  esc cancel"
	if m.notice != "" {
		hint = m.notice
	}
	c.put(line(r, r.Dy()-1), p.style(p.muted).Render(hint))
	return cursor
}
