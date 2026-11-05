package search

import (
	"fmt"
	"image/color"
	"os"
	"path/filepath"
	"strings"
	"unicode"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"
	"github.com/clipperhouse/uax29/v2/graphemes"
	"github.com/z0mbix/fmn/internal/history"
)

type layout struct {
	header, input, scope, results, inspector, footer uv.Rectangle
	stacked                                          bool
}

func geometry(width, height int, inspector bool) layout {
	w := width - 2
	chrome := 1
	if height >= 20 {
		chrome = 2
	}
	y := 1 + chrome
	bodyY := y + 3 + chrome
	bodyH := height - 1 - chrome - bodyY
	l := layout{
		header: uv.Rect(1, 1, w, chrome), input: uv.Rect(1, y, w, 3),
		scope: uv.Rect(1, y+3, w, chrome), results: uv.Rect(1, bodyY, w, bodyH),
		footer: uv.Rect(1, height-1-chrome, w, chrome),
	}
	switch {
	case inspector && w >= 96 && bodyH >= 10:
		split := w * 60 / 100
		l.results.Max.X = 1 + split
		l.inspector = uv.Rect(1+split, bodyY, w-split, bodyH)
	case inspector && bodyH >= 16:
		l.stacked = true
		l.results.Max.Y -= 8
		l.inspector = uv.Rect(1, l.results.Max.Y, w, 8)
	}
	return l
}

func resultRows(r uv.Rectangle) (uv.Rectangle, int) {
	r.Min.Y += 1
	if r.Dy() >= 4 {
		r.Min.Y++
	}
	step := 1
	if r.Dy() >= 8 {
		step = 2
	}
	return r, step
}

func visibleStart(selected, length, rows int) int {
	return min(max(0, selected-rows+1), max(0, length-rows))
}

type canvas struct{ uv.ScreenBuffer }

func (c canvas) fill(r uv.Rectangle, bg color.Color, reverse bool) {
	style := uv.Style{Bg: bg}
	if reverse {
		style.Attrs = uv.AttrReverse
	}
	c.FillArea(&uv.Cell{Content: " ", Width: 1, Style: style}, r.Intersect(c.Bounds()))
}

func (c canvas) put(r uv.Rectangle, text string) {
	r = r.Intersect(c.Bounds())
	if r.Empty() {
		return
	}
	width := min(ansi.StringWidth(text), r.Dx())
	if width > 0 {
		uv.NewStyledString(ansi.Truncate(text, width, "…")).Draw(&c, uv.Rect(r.Min.X, r.Min.Y, width, 1))
	}
}

func (c canvas) right(r uv.Rectangle, style lipgloss.Style, text string) {
	if w := ansi.StringWidth(text); w <= r.Dx() {
		c.put(uv.Rect(r.Max.X-w, r.Min.Y, w, 1), style.Render(text))
	}
}

func line(r uv.Rectangle, row int) uv.Rectangle {
	if row < 0 || row >= r.Dy() {
		return uv.Rectangle{}
	}
	return uv.Rect(r.Min.X, r.Min.Y+row, r.Dx(), 1)
}

func (c canvas) box(r uv.Rectangle, bg, border color.Color) {
	if r.Dx() < 2 || r.Dy() < 2 {
		return
	}
	style := lipgloss.NewStyle().Background(bg).Foreground(border)
	edge := strings.Repeat("─", r.Dx()-2)
	c.put(line(r, 0), style.Render("╭"+edge+"╮"))
	c.put(line(r, r.Dy()-1), style.Render("╰"+edge+"╯"))
	for y := r.Min.Y + 1; y < r.Max.Y-1; y++ {
		c.put(uv.Rect(r.Min.X, y, 1, 1), style.Render("│"))
		c.put(uv.Rect(r.Max.X-1, y, 1, 1), style.Render("│"))
	}
}

func fit(text string, width int) string {
	if width <= 0 {
		return ""
	}
	return ansi.Truncate(text, width, "…")
}

func shortPath(path string) string {
	if home, err := os.UserHomeDir(); err == nil {
		if path == home {
			path = "~"
		} else if suffix, ok := strings.CutPrefix(path, home+string(filepath.Separator)); ok {
			path = "~/" + suffix
		}
	}
	return displayText(path)
}

func (m model) age(timestamp int64) string {
	seconds := max(0, (float64(m.now)-float64(timestamp))/1000)
	switch {
	case seconds < 60:
		return "just now"
	case seconds < 3600:
		return fmt.Sprintf("%dm ago", int64(seconds/60))
	case seconds < 86400:
		return fmt.Sprintf("%dh ago", int64(seconds/3600))
	default:
		return fmt.Sprintf("%dd ago", int64(seconds/86400))
	}
}

func duration(ms *int64) string {
	switch {
	case ms == nil || *ms < 0:
		return "—"
	case *ms < 1000:
		return fmt.Sprintf("%dms", *ms)
	case *ms < 60000:
		return fmt.Sprintf("%.1fs", float64(*ms)/1000)
	case *ms < 3600000:
		return fmt.Sprintf("%dm %ds", *ms/60000, *ms/1000%60)
	default:
		return fmt.Sprintf("%dh %dm", *ms/3600000, *ms/60000%60)
	}
}

func (p palette) outcome(code *int32) (string, color.Color) {
	if code == nil {
		return "·", p.muted
	}
	if *code == 0 {
		return "✓", p.success
	}
	return "✗", p.failure
}

func highlighted(text string, indices []int, p palette, reverse bool) string {
	normal := p.style(p.text).Reverse(reverse)
	if len(indices) == 0 {
		return normal.Render(displayText(text))
	}
	match := p.style(p.match).Reverse(reverse).Bold(true).Underline(true)
	var output strings.Builder
	iter := graphemes.FromString(text)
	offset, next := 0, 0
	for iter.Next() {
		g := iter.Value()
		end := offset + len(g)
		for next < len(indices) && indices[next] < offset {
			next++
		}
		style := normal
		if next < len(indices) && indices[next] < end {
			style = match
		}
		output.WriteString(style.Render(displayText(g)))
		offset = end
	}
	return output.String()
}

func drawInput(c canvas, r uv.Rectangle, input textinput.Model, p palette, active bool) *tea.Cursor {
	promptWidth := ansi.StringWidth(input.Prompt)
	available := r.Dx() - promptWidth - 1
	if r.Empty() || available <= 0 {
		return nil
	}
	before, after := splitInput(input)
	before, after = displayText(before), displayText(after)
	if width := ansi.StringWidth(before); width >= available {
		before = ansi.TruncateLeft(before, width-available+1, "")
	}
	text, fg := before+after, p.text
	if text == "" {
		text, fg = input.Placeholder, p.muted
	}
	c.put(r, p.style(p.accent).Render(input.Prompt)+p.style(fg).Render(fit(text, available)))
	if !active {
		return nil
	}
	return tea.NewCursor(r.Min.X+promptWidth+ansi.StringWidth(before), r.Min.Y)
}

func displayText(text string) string {
	var safe strings.Builder
	for _, r := range text {
		switch {
		case r == '\n':
			safe.WriteString(" ↵ ")
		case r == '\t':
			safe.WriteString("    ")
		case r == '\x1b':
			safe.WriteString("␛")
		case unicode.IsControl(r):
			safe.WriteRune('�')
		default:
			safe.WriteRune(r)
		}
	}
	return safe.String()
}

func (m model) View() tea.View {
	c := canvas{uv.NewScreenBuffer(m.width, m.height)}
	p := themes[m.theme].palette
	c.fill(c.Bounds(), p.background, false)
	cursor := m.drawFocus(c, p)
	if m.screen != mainScreen {
		cursor = m.drawOverlay(c, p)
	}
	view := tea.NewView(c.Render())
	view.AltScreen, view.MouseMode, view.Cursor = true, tea.MouseModeCellMotion, cursor
	return view
}

func (m model) drawFocus(c canvas, p palette) *tea.Cursor {
	style := p.style(p.text)
	muted, accent, border := p.style(p.muted), p.style(p.accent), p.style(p.border)
	if m.width < 30 || m.height < 10 {
		c.put(line(c.Bounds(), 0), accent.Render("fmn › ")+style.Render(displayText(m.query.Value())))
		if selected := m.selected(); selected != nil {
			c.put(line(c.Bounds(), 1), style.Reverse(true).Render(displayText(selected.Command)))
		}
		c.put(line(c.Bounds(), 2), muted.Render("↵ "+m.acceptLabel()+"  esc cancel"))
		return nil
	}

	l := geometry(m.width, m.height, m.showInspector)
	c.box(c.Bounds(), p.background, p.border)
	c.put(line(l.header, 0), accent.Bold(true).Render(" fmn")+muted.Render(" / history"))
	if l.header.Dx() >= 48 {
		c.right(line(l.header, 0), muted, fit(shortPath(m.context.Cwd), l.header.Dx()-22)+" ")
	}
	if l.header.Dy() > 1 {
		c.put(line(l.header, 1), border.Render(strings.Repeat("─", l.header.Dx())))
	}
	c.box(l.input, p.background, p.accent)
	cursor := drawInput(c, uv.Rect(l.input.Min.X+1, l.input.Min.Y+1, l.input.Dx()-2, 1), m.query, p, m.screen == mainScreen)
	c.put(line(l.scope, 0), accent.Render(" ◉ "+m.scope.Label()))
	count := "matches"
	if len(m.matches) == 1 {
		count = "match"
	}
	c.right(line(l.scope, 0), muted, fmt.Sprintf("%d %s ", len(m.matches), count))

	m.drawResults(c, l.results, p)
	if !l.inspector.Empty() {
		m.drawInspector(c, l.inspector, p, l.stacked)
	}
	m.drawFooter(c, l.footer, p)
	return cursor
}

func (m model) drawResults(c canvas, r uv.Rectangle, p palette) {
	muted := p.style(p.muted)
	c.put(line(r, 0), muted.Render(" COMMANDS"))
	if r.Dx() >= 40 {
		c.right(line(r, 0), muted, "LAST USED ")
	}
	rows, step := resultRows(r)
	if len(m.matches) == 0 {
		message := "No matches. Try another query or change directory scope."
		if len(m.candidates) == 0 {
			message = "Your history starts here. Run a command, then Ctrl-R."
		}
		c.put(line(rows, 0), muted.Render(" "+message))
		return
	}
	n := rows.Dy() / step
	first := visibleStart(m.cursor, len(m.matches), n)
	for i := first; i < min(first+n, len(m.matches)); i++ {
		result := m.matches[i]
		candidate := m.candidates[result.index]
		area := uv.Rect(rows.Min.X, rows.Min.Y+(i-first)*step, rows.Dx(), step)
		current, mark, reverse := p, "  ", false
		if i == m.cursor {
			current, mark, reverse = p.selection(), " ›", p.selected == nil
			c.fill(area, current.background, reverse)
		}
		age, ageWidth := m.age(candidate.LastSeenAt), 0
		if area.Dx() >= 40 {
			ageWidth = ansi.StringWidth(age) + 2
			c.right(line(area, 0), current.style(current.muted).Reverse(reverse), age+" ")
		}
		c.put(line(area, 0), current.style(current.accent).Reverse(reverse).Render(mark))
		c.put(uv.Rect(area.Min.X+3, area.Min.Y, area.Dx()-4-ageWidth, 1),
			highlighted(candidate.Command, result.indices, current, reverse))
		if step == 2 {
			symbol, color := current.outcome(candidate.ExitCode)
			meta := current.style(color).Reverse(reverse).Render(symbol+" ") +
				current.style(current.muted).Reverse(reverse).Render(fmt.Sprintf("%s · ×%d", fit(shortPath(candidate.LatestCwd), area.Dx()-20), candidate.UseCount))
			c.put(uv.Rect(area.Min.X+3, area.Min.Y+1, area.Dx()-4, 1), meta)
		}
	}
}

func (m model) drawInspector(c canvas, r uv.Rectangle, p palette, stacked bool) {
	p.background = p.panel
	c.fill(r, p.background, false)
	border := p.style(p.border)
	if stacked {
		c.put(line(r, 0), border.Render(strings.Repeat("─", r.Dx())))
		r.Min.Y++
	} else {
		for y := r.Min.Y; y < r.Max.Y; y++ {
			c.put(uv.Rect(r.Min.X, y, 1, 1), border.Render("│"))
		}
		r.Min.X++
	}
	r.Min.X++
	r.Max.X--
	muted, accent := p.style(p.muted), p.style(p.accent)
	c.put(line(r, 0), muted.Render("INSPECTOR"))
	c.right(line(r, 0), accent, m.bindings.label(Details)+" expand")
	selected := m.selected()
	if selected == nil {
		c.put(line(r, 2), muted.Render("Select a command to inspect."))
		return
	}
	var ms *int64
	if len(m.preview) != 0 {
		ms = m.preview[0].DurationMS
	}
	if r.Dy() < 12 {
		c.put(line(r, 2), accent.Render(displayText(selected.Command)))
		c.put(line(r, 3), muted.Render(shortPath(selected.LatestCwd)))
		c.put(line(r, 4), p.style(p.text).Render(fmt.Sprintf("%d runs · %s · %s", selected.UseCount, m.age(selected.LastSeenAt), duration(ms))))
		return
	}
	command := strings.Split(selected.Command, "\n")
	var chunks []string
	for _, item := range command {
		text := displayText(item)
		chunks = append(chunks, strings.Split(ansi.Hardwrap(text, max(1, r.Dx()-2), false), "\n")...)
		if len(chunks) >= 4 {
			chunks = chunks[:4]
			break
		}
	}
	h := len(chunks) + 2
	c.box(uv.Rect(r.Min.X, r.Min.Y+2, r.Dx(), h), p.background, p.border)
	for i, text := range chunks {
		c.put(uv.Rect(r.Min.X+1, r.Min.Y+3+i, r.Dx()-2, 1), accent.Render(text))
	}
	exit := "running / unknown"
	if selected.ExitCode != nil {
		exit = fmt.Sprint(*selected.ExitCode)
	}
	symbol, color := p.outcome(selected.ExitCode)
	uses := fmt.Sprintf("%d times", selected.UseCount)
	if selected.UseCount == 1 {
		uses = "1 time"
	}
	facts := []struct{ label, value string }{
		{"Directory", shortPath(selected.LatestCwd)}, {"Last run", m.age(selected.LastSeenAt)},
		{"Duration", duration(ms)}, {"Exit code", symbol + " " + exit}, {"Used", uses},
	}
	for i, fact := range facts {
		fg := p.text
		if i == 3 {
			fg = color
		}
		c.put(line(r, h+3+i), muted.Render(fmt.Sprintf("%-10s", fact.label))+p.style(fg).Render(fact.value))
	}
	start := h + 10
	if r.Dy() > start+1 {
		c.put(line(r, start), muted.Render("RECENT OCCURRENCES"))
		for i, entry := range m.preview {
			y := start + 2 + i*3
			symbol, fg := p.outcome(entry.ExitCode)
			c.put(line(r, y), p.style(fg).Render(symbol+" ")+p.style(p.text).Render(m.age(entry.StartedAt)))
			c.right(line(r, y), muted, duration(entry.DurationMS))
			c.put(line(r, y+1), muted.Render("  "+shortPath(entry.Cwd)+" · "+displayText(entry.Shell)))
		}
	}
}

func (m model) drawFooter(c canvas, r uv.Rectangle, p palette) {
	y := r.Dy() - 1
	if y > 0 {
		c.put(line(r, 0), p.style(p.border).Render(strings.Repeat("─", r.Dx())))
	}
	accent, muted := p.style(p.accent), p.style(p.muted)
	items := [][2]string{{"↵", m.acceptLabel()}, {m.bindings.label(Actions), "actions"}, {m.bindings.label(Help), "help"}, {"esc", "cancel"}}
	if r.Dx() >= 62 {
		items = [][2]string{{"↑↓", "move"}, {"↵", m.acceptLabel()}, {m.bindings.label(ScopeCycle), "scope"},
			{m.bindings.label(Details), "details"}, {m.bindings.label(Actions), "actions"}}
		if r.Dx() >= 92 {
			items[0][1] = "navigate"
			items = append(items, [2]string{m.bindings.label(Statistics), "stats"}, [2]string{m.bindings.label(Settings), "settings"})
		}
		items = append(items, [2]string{m.bindings.label(Help), "help"})
	}
	text := muted.Render(" ")
	for _, item := range items {
		text += accent.Render(item[0]) + muted.Render(" "+item[1]+"  ")
	}
	c.put(line(r, y), text)
	label := themes[m.theme].label + " · esc cancel "
	if m.notice != "" {
		label = m.notice + " "
	}
	if r.Dx()-ansi.StringWidth(text) >= ansi.StringWidth(label) {
		c.right(line(r, y), muted, label)
	}
}

func (m model) acceptLabel() string {
	if m.execute {
		return "run"
	}
	return "insert"
}

var scopeOrder = [...]history.Scope{history.Global, history.Directory, history.Repository, history.Session}
