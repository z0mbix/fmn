// Package search implements the interactive history picker.
package search

import (
	"context"
	"errors"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/term"
	"github.com/z0mbix/fmn/internal/history"
)

type Options struct {
	Query, Cwd string
	Preferences
	Widget, NoColor bool
	Save            func(Change) error
}

type Selection struct {
	Command           string
	Accepted, Execute bool
}

type Interruption syscall.Signal

func (s Interruption) Error() string { return syscall.Signal(s).String() }

func Run(store *history.Store, input, output *os.File, options Options) (Selection, error) {
	if !term.IsTerminal(input.Fd()) || !term.IsTerminal(output.Fd()) {
		return Selection{}, errors.New("search requires an interactive terminal")
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGHUP, syscall.SIGTERM)
	defer signal.Stop(signals)
	received := make(chan Interruption, 1)
	go func() {
		select {
		case sig := <-signals:
			received <- Interruption(sig.(syscall.Signal))
			cancel()
		case <-ctx.Done():
		}
	}()

	initial, err := newModel(store, options)
	if err != nil {
		return Selection{}, err
	}
	var environment []string
	for _, item := range os.Environ() {
		// NO_COLOR chooses the initial palette. Previewing another theme can
		// still use the terminal's full colour capabilities.
		if !strings.HasPrefix(item, "NO_COLOR=") {
			environment = append(environment, item)
		}
	}
	final, err := tea.NewProgram(initial,
		tea.WithInput(input), tea.WithOutput(output), tea.WithContext(ctx),
		tea.WithoutSignalHandler(), tea.WithEnvironment(environment),
	).Run()
	if ctx.Err() != nil {
		return Selection{}, <-received
	}
	if err != nil {
		return Selection{}, err
	}
	result := final.(model)
	return Selection{result.command, result.accepted, result.execute}, result.err
}

type screen int

const (
	mainScreen screen = iota
	themeScreen
	detailsScreen
	helpScreen
	actionsScreen
	statsScreen
	settingsScreen
	bindingsScreen
	confirmScreen
)

type model struct {
	store         *history.Store
	context       history.SearchContext
	scope         history.Scope
	query         textinput.Model
	candidates    []history.Candidate
	matches       []ranked
	cursor        int
	preview       []history.Entry
	previewID     int64
	width, height int
	now           int64
	showInspector bool
	execute       bool
	widget        bool
	prefs         Preferences
	bindings      Bindings
	save          func(Change) error
	theme         int
	screen        screen
	themeBack     screen
	themeBefore   int
	themeQuery    textinput.Model
	menuCursor    int
	settingCursor int
	settingsBack  screen
	statsBack     screen
	helpBack      screen
	detailCursor  int
	details       []history.Entry
	overlayBack   screen
	stats         history.Stats
	confirm       confirmation
	captureKey    bool
	notice        string
	command       string
	accepted      bool
	err           error
}

func newInput(prompt, placeholder string) textinput.Model {
	input := textinput.New()
	input.Prompt, input.Placeholder = prompt, placeholder
	input.SetVirtualCursor(false)
	input.Focus()
	return input
}

func newModel(store *history.Store, options Options) (model, error) {
	scopeContext := history.DetectSearchContext(options.Cwd)
	m := model{
		store: store, context: scopeContext, scope: scopeContext.Effective(options.DefaultScope),
		query: newInput(" › ", "Search your command history"), width: 80, height: 24,
		showInspector: options.ShowInspector, execute: options.Widget && options.ExecuteOnSelect,
		widget: options.Widget, prefs: options.Preferences, save: options.Save, bindings: options.Bindings,
		now: time.Now().UnixMilli(), theme: max(0, themeIndex(options.Theme)),
	}
	if m.bindings == (Bindings{}) {
		m.bindings = DefaultBindings()
	}
	if options.NoColor {
		m.theme = themeIndex("no-color")
	}
	m.query.SetValue(options.Query)
	m.query.CursorEnd()
	err := m.reload()
	return m, err
}

func (m model) Init() tea.Cmd { return nil }

func (m *model) reload() error {
	candidates, err := m.store.Candidates(m.context, m.scope)
	if err != nil {
		return err
	}
	m.candidates = candidates
	m.previewID = 0
	m.refilter()
	return m.err
}

func (m *model) refilter() {
	m.matches = rank(m.candidates, m.query.Value(), m.now)
	m.cursor = 0
	m.refreshPreview()
}

func (m model) selected() *history.Candidate {
	if m.cursor < 0 || m.cursor >= len(m.matches) {
		return nil
	}
	return &m.candidates[m.matches[m.cursor].index]
}

func (m *model) refreshPreview() {
	selected := m.selected()
	if selected == nil || !m.showInspector {
		m.previewID, m.preview = 0, nil
		return
	}
	if selected.ID != m.previewID {
		m.preview, m.err = m.store.Occurrences(selected.ID, 3, m.context, m.scope)
		m.previewID = selected.ID
	}
}

func (m *model) persist(change Change) bool {
	if m.save != nil {
		if err := m.save(change); err != nil {
			m.notice = "Could not save: " + err.Error()
			return false
		}
	}
	m.notice = ""
	return true
}

func (m model) quit() (tea.Model, tea.Cmd) {
	return m, tea.Quit
}

func (m model) accept(command string) (tea.Model, tea.Cmd) {
	m.command, m.accepted = command, true
	return m.quit()
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = max(1, msg.Width), max(1, msg.Height)
		m.query.SetWidth(max(1, m.width-8))
		return m, nil
	case tea.KeyPressMsg:
		if msg.String() == "ctrl+c" {
			return m.quit()
		}
	}
	if m.screen != mainScreen {
		return m.updateOverlay(msg)
	}
	switch msg := msg.(type) {
	case tea.MouseClickMsg:
		r, step := resultRows(geometry(m.width, m.height, m.showInspector).results)
		mouse := msg.Mouse()
		if mouse.Button == tea.MouseLeft && m.width >= 30 && m.height >= 10 &&
			mouse.X >= r.Min.X && mouse.X < r.Max.X && mouse.Y >= r.Min.Y && mouse.Y < r.Max.Y {
			index := visibleStart(m.cursor, len(m.matches), r.Dy()/step) + (mouse.Y-r.Min.Y)/step
			if index < len(m.matches) {
				m.cursor = index
				m.refreshPreview()
			}
		}
		return m.checked()
	case tea.MouseWheelMsg:
		rows, _ := resultRows(geometry(m.width, m.height, m.showInspector).results)
		if inRows(msg, rows) {
			m.move(wheelDelta(msg))
		}
		return m.checked()
	case tea.KeyPressMsg:
		key := msg.String()
		switch key {
		case "esc":
			return m.quit()
		case "enter":
			if item := m.selected(); item != nil {
				return m.accept(item.Command)
			}
			return m.quit()
		case "up", "ctrl+p":
			m.move(-1)
		case "down", "ctrl+n":
			m.move(1)
		case "pgup":
			m.move(-m.visibleRows())
		case "pgdown":
			m.move(m.visibleRows())
		case "ctrl+u":
			m.query.Reset()
			m.refilter()
		case m.bindings[ScopeCycle], "shift+tab":
			m.scope = m.context.Next(m.scope, key == "shift+tab")
			m.err = m.reload()
		case m.bindings[Inspector]:
			m.toggleInspector()
		case m.bindings[Themes]:
			m.openThemes(mainScreen)
		case m.bindings[Details]:
			m.openDetails()
		case m.bindings[Help]:
			m.screen, m.helpBack = helpScreen, mainScreen
		case m.bindings[Actions]:
			m.openActions()
		case m.bindings[Statistics]:
			m.openStats(mainScreen)
		case m.bindings[Settings]:
			m.openSettings(mainScreen)
		default:
			goto edit
		}
		return m.checked()
	}
edit:
	old := m.query.Value()
	var cmd tea.Cmd
	m.query, cmd = updateInput(m.query, msg)
	if old != m.query.Value() {
		m.refilter()
	}
	if m.err != nil {
		return m.quit()
	}
	return m, cmd
}

func (m *model) toggleInspector() {
	value := !m.showInspector
	if m.persist(Change{"show_inspector", value}) {
		m.showInspector, m.prefs.ShowInspector = value, value
		m.refreshPreview()
	}
}

func (m model) checked() (tea.Model, tea.Cmd) {
	if m.err != nil {
		return m.quit()
	}
	return m, nil
}

func (m *model) move(delta int) {
	m.cursor = min(max(0, m.cursor+delta), max(0, len(m.matches)-1))
	m.refreshPreview()
}

func (m model) visibleRows() int {
	if m.width < 30 || m.height < 10 {
		return 1
	}
	r, step := resultRows(geometry(m.width, m.height, m.showInspector).results)
	return max(1, r.Dy()/step)
}

func wheelDelta(msg tea.MouseWheelMsg) int {
	switch msg.Button {
	case tea.MouseWheelUp:
		return -1
	case tea.MouseWheelDown:
		return 1
	default:
		return 0
	}
}
