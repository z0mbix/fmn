package search

import (
	tea "charm.land/bubbletea/v2"
	"fmt"
	uv "github.com/charmbracelet/ultraviolet"
)

func (m model) dialog(c canvas, p palette, width, height int, title string) (uv.Rectangle, palette) {
	r := m.popup(width, height)
	p.background = p.panel
	c.fill(r, p.background, false)
	c.box(r, p.background, p.accent)
	c.put(uv.Rect(r.Min.X+2, r.Min.Y, max(0, r.Dx()-4), 1), p.style(p.text).Render(" "+title+" "))
	return uv.Rect(r.Min.X+2, r.Min.Y+1, r.Dx()-4, r.Dy()-2), p
}

func (m model) popup(width, height int) uv.Rectangle {
	width = min(width, max(1, m.width-2))
	height = min(height, max(1, m.height-2))
	return uv.Rect((m.width-width)/2, (m.height-height)/2, width, height)
}

func (m *model) openMenu(screen screen) {
	m.screen, m.menuCursor, m.notice = screen, 0, ""
}

func (m *model) openThemes(back screen) {
	m.screen, m.themeBack, m.themeBefore = themeScreen, back, m.theme
	m.themeQuery = newInput("Filter › ", "type to find a theme")
	m.menuCursor, m.notice = m.theme, ""
}

func (m model) themeRows() uv.Rectangle {
	r := m.popup(64, len(themes)+7)
	return uv.Rect(r.Min.X+2, r.Min.Y+4, max(0, r.Dx()-4), max(0, r.Dy()-6))
}

func (m *model) previewTheme(index int) {
	matches := matchThemes(m.themeQuery.Value())
	if len(matches) == 0 {
		m.menuCursor = 0
		return
	}
	m.menuCursor = min(max(0, index), len(matches)-1)
	m.theme = matches[m.menuCursor].index
}

func (m model) applyTheme() (tea.Model, tea.Cmd) {
	if len(matchThemes(m.themeQuery.Value())) == 0 {
		return m, nil
	}
	key := themes[m.theme].name
	if m.persist(Change{"theme", key}) {
		m.prefs.Theme, m.screen = key, m.themeBack
	}
	return m, nil
}

func (m model) updateTheme(msg tea.Msg) (tea.Model, tea.Cmd) {
	rows := m.themeRows()
	matches := matchThemes(m.themeQuery.Value())
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "esc":
			m.theme, m.screen, m.notice = m.themeBefore, m.themeBack, ""
		case "enter":
			return m.applyTheme()
		case "up", "ctrl+p":
			m.previewTheme(m.menuCursor - 1)
		case "down", "ctrl+n":
			m.previewTheme(m.menuCursor + 1)
		case "pgup":
			m.previewTheme(m.menuCursor - max(1, rows.Dy()))
		case "pgdown":
			m.previewTheme(m.menuCursor + max(1, rows.Dy()))
		case "home":
			m.previewTheme(0)
		case "end":
			m.previewTheme(len(matches) - 1)
		case "ctrl+u":
			m.themeQuery.Reset()
			m.restoreThemeFocus()
		default:
			goto edit
		}
		return m, nil
	case tea.MouseWheelMsg:
		if inRows(msg, rows) {
			m.previewTheme(m.menuCursor + wheelDelta(msg))
		}
		return m, nil
	case tea.MouseClickMsg:
		if i := clickedRow(msg, rows, m.menuCursor, len(matches)); i >= 0 {
			m.previewTheme(i)
			return m.applyTheme()
		}
		return m, nil
	}
edit:
	old := m.themeQuery.Value()
	m.themeQuery.SetWidth(max(1, rows.Dx()-4))
	var cmd tea.Cmd
	m.themeQuery, cmd = updateInput(m.themeQuery, msg)
	if m.themeQuery.Value() != old {
		if m.themeQuery.Value() == "" {
			m.restoreThemeFocus()
		} else {
			m.previewTheme(0)
		}
	}
	return m, cmd
}

func (m *model) restoreThemeFocus() {
	m.menuCursor = m.theme
}

func inRows(msg tea.MouseMsg, r uv.Rectangle) bool {
	mouse := msg.Mouse()
	return uv.Pos(mouse.X, mouse.Y).In(r)
}

func clickedRow(msg tea.MouseClickMsg, r uv.Rectangle, selected, length int) int {
	m := msg.Mouse()
	if m.Button == tea.MouseLeft && inRows(msg, r) {
		index := visibleStart(selected, length, r.Dy()) + m.Y - r.Min.Y
		if index < length {
			return index
		}
	}
	return -1
}

func (m model) drawThemes(c canvas, p palette) *tea.Cursor {
	inside, p := m.dialog(c, p, 64, len(themes)+7, "Colour theme")
	cursor := drawInput(c, line(inside, 1), m.themeQuery, p, true)
	rows := m.themeRows()
	matches := matchThemes(m.themeQuery.Value())
	first := visibleStart(m.menuCursor, len(matches), rows.Dy())
	if len(matches) == 0 {
		c.put(line(rows, 0), p.style(p.muted).Render("No matching themes"))
	}
	for i := first; i < min(first+rows.Dy(), len(matches)); i++ {
		match := matches[i]
		item := themes[match.index]
		row := line(rows, i-first)
		mark, reverse, current := "  ", false, p
		if i == m.menuCursor {
			mark, reverse, current = "› ", p.selected == nil, p.selection()
			c.fill(row, current.background, reverse)
		}
		c.put(row, current.style(current.accent).Reverse(reverse).Render(mark)+highlighted(item.label, match.indices, current, reverse))
		if row.Dx() >= 29 {
			swatch := item.palette.accent
			if p.accent == nil || i == m.menuCursor {
				swatch = current.accent
			}
			c.right(row, current.style(swatch).Reverse(reverse), "● ● ●")
		}
	}
	index := 0
	if len(matches) > 0 {
		index = m.menuCursor + 1
	}
	hint := fmt.Sprintf("↑↓ preview · ↵ save · ^U clear · esc  %d/%d", index, len(matches))
	if m.notice != "" {
		hint = m.notice
	}
	c.put(line(inside, inside.Dy()-1), p.style(p.muted).Render(hint))
	return cursor
}

func (c canvas) dim(p palette) {
	for y := 0; y < c.Height(); y++ {
		for x := 0; x < c.Width(); x++ {
			if cell := c.CellAt(x, y); cell != nil && cell.Width > 0 {
				cell.Style.Fg = p.muted
				cell.Style.Attrs |= uv.AttrFaint
			}
		}
	}
}
