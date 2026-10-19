use std::io::{self, IsTerminal, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Position, Rect};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::{Theme, UiConfig};
use crate::model::{Entry, SearchCandidate};
use crate::search::{RankedMatch, Ranker};
use crate::storage::Store;
use crate::{Error, Result};

mod render;
mod theme;

use render::draw;
use theme::THEMES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    DeleteEntry(i64),
    ForgetCommand(i64),
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    None,
    Accept(String),
    Cancel,
    LoadDetails(i64),
    Mutate(Mutation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum View {
    Main,
    Details,
    Help,
    Manage,
    Themes,
    Confirm { mutation: Mutation, input: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Insert,
    Inspect,
    Theme,
    Delete,
    Forget,
    Clear,
}

#[derive(Debug)]
pub struct App {
    query: String,
    cursor: usize,
    candidates: Vec<SearchCandidate>,
    ranked: Vec<RankedMatch>,
    ranker: Ranker,
    selected: usize,
    current_dir_only: bool,
    view: View,
    details: Vec<Entry>,
    detail_selected: usize,
    preview: Vec<Entry>,
    preview_command_id: Option<i64>,
    return_to_details: bool,
    action_selected: usize,
    theme: Theme,
    previous_theme: Theme,
    show_inspector: bool,
    execute_on_select: bool,
    current_dir: String,
    home_dir: Option<String>,
    now_millis: i64,
}

impl App {
    pub fn new(candidates: Vec<SearchCandidate>, query: String, now_millis: i64) -> Self {
        let mut ranker = Ranker::default();
        let mut ranked = Vec::new();
        ranker.rank_into(&candidates, &query, false, now_millis, &mut ranked);
        let cursor = query.len();
        Self {
            query,
            cursor,
            candidates,
            ranked,
            ranker,
            selected: 0,
            current_dir_only: false,
            view: View::Main,
            details: Vec::new(),
            detail_selected: 0,
            preview: Vec::new(),
            preview_command_id: None,
            return_to_details: false,
            action_selected: 0,
            theme: Theme::default(),
            previous_theme: Theme::default(),
            show_inspector: true,
            execute_on_select: false,
            current_dir: String::new(),
            home_dir: std::env::var("HOME").ok(),
            now_millis,
        }
    }

    pub fn set_candidates(&mut self, candidates: Vec<SearchCandidate>) {
        self.candidates = candidates;
        self.refresh();
    }

    pub fn show_details(&mut self, entries: Vec<Entry>) {
        let command_id = self.selected_candidate().map(|candidate| candidate.id);
        self.details = entries
            .into_iter()
            .filter(|entry| Some(entry.command_id) == command_id)
            .collect();
        self.detail_selected = 0;
        self.view = View::Details;
        self.return_to_details = false;
    }

    fn set_preview(&mut self, command_id: i64, entries: Vec<Entry>) {
        if self.selected_candidate().map(|candidate| candidate.id) == Some(command_id) {
            self.preview = entries
                .into_iter()
                .filter(|entry| entry.command_id == command_id)
                .take(3)
                .collect();
            self.preview_command_id = Some(command_id);
        }
    }

    fn open_overlay(&mut self, view: View) {
        if matches!(self.view, View::Main | View::Details) {
            self.return_to_details = self.view == View::Details;
        }
        self.action_selected = 0;
        if view == View::Themes {
            self.previous_theme = self.theme;
        }
        self.view = view;
    }

    fn close_overlay(&mut self) {
        self.view = if self.return_to_details {
            View::Details
        } else {
            View::Main
        };
        self.return_to_details = false;
    }

    fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::Insert, Action::Inspect, Action::Theme];
        if self.selected_detail().is_some() {
            actions.push(Action::Delete);
        }
        actions.extend([Action::Forget, Action::Clear]);
        actions
    }

    fn activate_action(&mut self, action: Action) -> Intent {
        match action {
            Action::Insert => self
                .selected_candidate()
                .map(|candidate| Intent::Accept(candidate.command.clone()))
                .unwrap_or(Intent::None),
            Action::Inspect => self
                .selected_candidate()
                .map(|candidate| Intent::LoadDetails(candidate.id))
                .unwrap_or(Intent::None),
            Action::Theme => {
                self.open_overlay(View::Themes);
                Intent::None
            }
            action => {
                let mutation = match action {
                    Action::Delete => self
                        .selected_detail()
                        .map(|entry| Mutation::DeleteEntry(entry.id)),
                    Action::Forget => self
                        .selected_candidate()
                        .map(|candidate| Mutation::ForgetCommand(candidate.id)),
                    Action::Clear => Some(Mutation::Clear),
                    _ => None,
                };
                if let Some(mutation) = mutation {
                    self.view = View::Confirm {
                        mutation,
                        input: String::new(),
                    };
                }
                Intent::None
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Intent {
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return Intent::Cancel;
        }

        match &mut self.view {
            View::Main => self.handle_main(key),
            View::Details => self.handle_details(key),
            View::Help | View::Manage => self.handle_overlay(key),
            View::Themes => {
                let index = THEMES
                    .iter()
                    .position(|(theme, _)| *theme == self.theme)
                    .unwrap_or(0);
                match key.code {
                    KeyCode::Esc => {
                        self.theme = self.previous_theme;
                        self.close_overlay();
                    }
                    KeyCode::Enter => self.close_overlay(),
                    KeyCode::Up => self.theme = THEMES[index.saturating_sub(1)].0,
                    KeyCode::Down => self.theme = THEMES[(index + 1).min(THEMES.len() - 1)].0,
                    _ => {}
                }
                Intent::None
            }
            View::Confirm { mutation, input } => {
                let mutation = mutation.clone();
                match key.code {
                    KeyCode::Esc => self.close_overlay(),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(character)
                        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                    {
                        input.push(character);
                    }
                    KeyCode::Enter if input == mutation_confirmation(&mutation) => {
                        self.view = View::Main;
                        self.details.clear();
                        self.preview.clear();
                        self.preview_command_id = None;
                        self.return_to_details = false;
                        return Intent::Mutate(mutation);
                    }
                    _ => {}
                }
                Intent::None
            }
        }
    }

    fn handle_main(&mut self, key: KeyEvent) -> Intent {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => Intent::Cancel,
            (KeyCode::Enter, _) => self
                .selected_candidate()
                .map(|candidate| Intent::Accept(candidate.command.clone()))
                .unwrap_or(Intent::Cancel),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.selected = self.selected.saturating_sub(1);
                Intent::None
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                if self.selected + 1 < self.ranked.len() {
                    self.selected += 1;
                }
                Intent::None
            }
            (KeyCode::Tab, _) => {
                self.current_dir_only = !self.current_dir_only;
                self.refresh();
                Intent::None
            }
            (KeyCode::F(2), _) => self
                .selected_candidate()
                .map(|candidate| Intent::LoadDetails(candidate.id))
                .unwrap_or(Intent::None),
            (KeyCode::Char('?'), KeyModifiers::CONTROL) | (KeyCode::F(1), _) => {
                self.open_overlay(View::Help);
                Intent::None
            }
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                self.open_overlay(View::Manage);
                Intent::None
            }
            (KeyCode::F(3), _) => {
                self.open_overlay(View::Themes);
                Intent::None
            }
            (KeyCode::F(4), _) => {
                self.show_inspector = !self.show_inspector;
                Intent::None
            }
            (KeyCode::Left, _) => {
                self.cursor = self.query[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                Intent::None
            }
            (KeyCode::Right, _) => {
                if let Some(grapheme) = self.query[self.cursor..].graphemes(true).next() {
                    self.cursor += grapheme.len();
                }
                Intent::None
            }
            (KeyCode::Home, _) => {
                self.cursor = 0;
                Intent::None
            }
            (KeyCode::End, _) => {
                self.cursor = self.query.len();
                Intent::None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.query.clear();
                self.cursor = 0;
                self.refresh();
                Intent::None
            }
            (KeyCode::Backspace, _) => {
                let previous = self.query[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                self.query.drain(previous..self.cursor);
                self.cursor = previous;
                self.refresh();
                Intent::None
            }
            (KeyCode::Delete, _) => {
                if let Some(grapheme) = self.query[self.cursor..].graphemes(true).next() {
                    let end = self.cursor + grapheme.len();
                    self.query.drain(self.cursor..end);
                }
                self.refresh();
                Intent::None
            }
            (KeyCode::Char(character), modifiers)
                if !character.is_control()
                    && (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT) =>
            {
                self.query.insert(self.cursor, character);
                self.cursor += character.len_utf8();
                self.refresh();
                Intent::None
            }
            _ => Intent::None,
        }
    }

    fn handle_details(&mut self, key: KeyEvent) -> Intent {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Left, _) => {
                self.details.clear();
                self.view = View::Main;
                Intent::None
            }
            (KeyCode::Enter, _) => self
                .details
                .get(self.detail_selected)
                .map(|entry| Intent::Accept(entry.command.clone()))
                .unwrap_or(Intent::None),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.detail_selected = self.detail_selected.saturating_sub(1);
                Intent::None
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                if self.detail_selected + 1 < self.details.len() {
                    self.detail_selected += 1;
                }
                Intent::None
            }
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                self.open_overlay(View::Manage);
                Intent::None
            }
            (KeyCode::F(1), _) => {
                self.open_overlay(View::Help);
                Intent::None
            }
            (KeyCode::F(3), _) => {
                self.open_overlay(View::Themes);
                Intent::None
            }
            _ => Intent::None,
        }
    }

    fn handle_overlay(&mut self, key: KeyEvent) -> Intent {
        if self.view == View::Help {
            self.close_overlay();
            return Intent::None;
        }
        match key.code {
            KeyCode::Esc => self.close_overlay(),
            KeyCode::Up => self.action_selected = self.action_selected.saturating_sub(1),
            KeyCode::Down => {
                self.action_selected = (self.action_selected + 1).min(self.actions().len() - 1)
            }
            KeyCode::Enter => return self.activate_action(self.actions()[self.action_selected]),
            KeyCode::Char('i') | KeyCode::F(2) => return self.activate_action(Action::Inspect),
            KeyCode::Char('t') | KeyCode::F(3) => return self.activate_action(Action::Theme),
            KeyCode::Char('d') => return self.activate_action(Action::Delete),
            KeyCode::Char('f') => return self.activate_action(Action::Forget),
            KeyCode::Char('c') => return self.activate_action(Action::Clear),
            _ => {}
        }
        Intent::None
    }

    fn selected_candidate(&self) -> Option<&SearchCandidate> {
        self.ranked
            .get(self.selected)
            .map(|ranked| &self.candidates[ranked.index])
    }

    fn selected_detail(&self) -> Option<&Entry> {
        if self.view != View::Details && !self.return_to_details {
            return None;
        }
        let command = self.selected_candidate()?;
        self.details
            .get(self.detail_selected)
            .filter(|entry| entry.command_id == command.id)
    }

    fn refresh(&mut self) {
        self.details.clear();
        self.detail_selected = 0;
        self.preview.clear();
        self.preview_command_id = None;
        self.ranker.rank_into(
            &self.candidates,
            &self.query,
            self.current_dir_only,
            self.now_millis,
            &mut self.ranked,
        );
        self.selected = self.selected.min(self.ranked.len().saturating_sub(1));
    }

    fn handle_paste(&mut self, text: &str) {
        if self.view == View::Main {
            let text: String = text
                .chars()
                .map(|c| if c.is_whitespace() { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            self.query.insert_str(self.cursor, &text);
            self.cursor += text.len();
            self.refresh();
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Intent {
        let position = Position::new(mouse.column, mouse.row);
        match self.view {
            View::Main => {
                let layout = render::geometry(area, self.show_inspector);
                let (rows, height) = render::candidate_rows(layout.results);
                if rows.contains(position) {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let range = visible_range(
                                self.selected,
                                self.ranked.len(),
                                rows.height / height,
                            );
                            let index = range.start + usize::from((mouse.row - rows.y) / height);
                            if index < range.end {
                                self.selected = index;
                            }
                        }
                        MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
                        MouseEventKind::ScrollDown => {
                            self.selected =
                                (self.selected + 1).min(self.ranked.len().saturating_sub(1))
                        }
                        _ => {}
                    }
                }
            }
            View::Details => {
                let rows = render::details_rows(area);
                if rows.contains(position) {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let range = visible_range(
                                self.detail_selected,
                                self.details.len(),
                                rows.height / 2,
                            );
                            let index = range.start + usize::from((mouse.row - rows.y) / 2);
                            if index < range.end {
                                self.detail_selected = index;
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            self.detail_selected = self.detail_selected.saturating_sub(1)
                        }
                        MouseEventKind::ScrollDown => {
                            self.detail_selected =
                                (self.detail_selected + 1).min(self.details.len().saturating_sub(1))
                        }
                        _ => {}
                    }
                }
            }
            View::Manage | View::Themes
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) =>
            {
                let rows = render::menu_rows(area, self);
                if rows.contains(position) {
                    let index = usize::from(mouse.row - rows.y);
                    if self.view == View::Manage && index < self.actions().len() {
                        self.action_selected = index;
                        return self.activate_action(self.actions()[index]);
                    }
                    if self.view == View::Themes && index < THEMES.len() {
                        self.theme = THEMES[index].0;
                        self.close_overlay();
                    }
                }
            }
            _ => {}
        }
        Intent::None
    }
}

pub fn run(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
) -> Result<Option<String>> {
    if !io::stderr().is_terminal() || !io::stdin().is_terminal() {
        return Err(Error::Io(io::Error::other(
            "search requires an interactive terminal",
        )));
    }

    let config = UiConfig {
        execute_on_select: false,
        ..config.clone()
    };
    run_with_backend(store, query, cwd, &config, io::stderr())
}

pub fn run_for_shell(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
) -> Result<Option<String>> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return Err(Error::Io(io::Error::other(
            "search requires an interactive terminal",
        )));
    }
    run_with_backend(store, query, cwd, config, io::stdout())
}

fn run_with_backend<W: Write>(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
    writer: W,
) -> Result<Option<String>> {
    let candidates = store.search_candidates(&cwd)?;
    let mut app = App::new(candidates, query, now_millis());
    app.theme = if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        Theme::NoColor
    } else {
        config.theme
    };
    app.show_inspector = config.show_inspector;
    app.execute_on_select = config.execute_on_select;
    app.current_dir = cwd.clone();
    let mut terminal = TuiTerminal::enter(writer)?;

    loop {
        if let Some(command_id) = app.selected_candidate().map(|candidate| candidate.id)
            && app.preview_command_id != Some(command_id)
        {
            app.set_preview(command_id, store.preview_entries_for_command(command_id)?);
        }
        terminal.terminal.draw(|frame| draw(frame, &app))?;
        let intent = match event::read()? {
            Event::Key(key) if key.is_press() => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse, terminal.terminal.get_frame().area()),
            Event::Paste(text) => {
                app.handle_paste(&text);
                Intent::None
            }
            _ => Intent::None,
        };
        match intent {
            Intent::None => {}
            Intent::Accept(command) => return Ok(Some(command)),
            Intent::Cancel => return Ok(None),
            Intent::LoadDetails(command_id) => {
                app.show_details(store.entries_for_command(command_id)?);
            }
            Intent::Mutate(mutation) => {
                match mutation {
                    Mutation::DeleteEntry(entry_id) => store.delete_entry(entry_id)?,
                    Mutation::ForgetCommand(command_id) => store.forget_command(command_id)?,
                    Mutation::Clear => store.clear()?,
                }
                app.set_candidates(store.search_candidates(&cwd)?);
            }
        }
    }
}

struct TuiTerminal<W: Write> {
    terminal: Terminal<CrosstermBackend<W>>,
}

impl<W: Write> TuiTerminal<W> {
    fn enter(writer: W) -> Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(writer))?;
        enable_raw_mode().map_err(|error| {
            Error::Io(io::Error::new(
                error.kind(),
                format!("could not enable terminal raw mode: {error}"),
            ))
        })?;
        let mut session = Self { terminal };
        execute!(
            session.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        session.terminal.hide_cursor()?;
        Ok(session)
    }
}

impl<W: Write> Drop for TuiTerminal<W> {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

fn visible_range(selected: usize, length: usize, height: u16) -> std::ops::Range<usize> {
    let start = selected
        .min(length.saturating_sub(1))
        .saturating_sub(usize::from(height).saturating_sub(1));
    start..length.min(start + usize::from(height))
}

fn mutation_confirmation(mutation: &Mutation) -> &'static str {
    match mutation {
        Mutation::DeleteEntry(_) => "delete",
        Mutation::ForgetCommand(_) => "forget",
        Mutation::Clear => "clear",
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::{App, Intent, Mutation, View, draw};
    use crate::model::{Entry, SearchCandidate};

    fn candidate(id: i64, command: &str, cwd_count: i64) -> SearchCandidate {
        SearchCandidate {
            id,
            command: command.into(),
            first_seen_at: 1,
            last_seen_at: 1,
            use_count: 1,
            cwd_use_count: cwd_count,
            cwd_last_seen_at: (cwd_count > 0).then_some(1),
            latest_cwd: "/tmp".into(),
            latest_exit_code: Some(0),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_filters_and_enter_accepts() {
        let mut app = App::new(
            vec![candidate(1, "git status", 1), candidate(2, "cargo test", 0)],
            String::new(),
            1,
        );
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::Accept("git status".into())
        );
    }

    #[test]
    fn escape_cancels_from_main() {
        let mut app = App::new(vec![], String::new(), 1);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Intent::Cancel);
    }

    #[test]
    fn ctrl_c_cancels_from_every_view() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = App::new(vec![candidate(1, "one", 1)], String::new(), 1);

        assert_eq!(app.handle_key(ctrl_c), Intent::Cancel);

        app.handle_key(key(KeyCode::F(1)));
        assert_eq!(app.handle_key(ctrl_c), Intent::Cancel);

        app.show_details(vec![]);
        assert_eq!(app.handle_key(ctrl_c), Intent::Cancel);

        app.view = View::Manage;
        assert_eq!(app.handle_key(ctrl_c), Intent::Cancel);

        app.view = View::Confirm {
            mutation: Mutation::Clear,
            input: String::new(),
        };
        assert_eq!(app.handle_key(ctrl_c), Intent::Cancel);
    }

    #[test]
    fn tab_toggles_strict_directory_filter() {
        let mut app = App::new(
            vec![candidate(1, "global", 0), candidate(2, "local", 1)],
            String::new(),
            1,
        );
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.ranked.len(), 1);
        assert_eq!(app.selected_candidate().unwrap().command, "local");
    }

    #[test]
    fn details_can_accept_specific_occurrence() {
        let mut app = App::new(vec![candidate(1, "one", 0)], String::new(), 1);
        assert_eq!(app.handle_key(key(KeyCode::F(2))), Intent::LoadDetails(1));
        app.show_details(vec![Entry {
            id: 9,
            command_id: 1,
            command: "one".into(),
            started_at: 1,
            finished_at: None,
            duration_ms: None,
            exit_code: None,
            cwd: "/".into(),
            shell: "fish".into(),
            session_id: "s".into(),
            source: "live".into(),
        }]);
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::Accept("one".into())
        );
    }

    #[test]
    fn destructive_action_requires_exact_confirmation() {
        let mut app = App::new(vec![candidate(4, "one", 0)], String::new(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(app.view, View::Manage);
        app.handle_key(key(KeyCode::Char('f')));
        for character in "forget".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::Mutate(Mutation::ForgetCommand(4))
        );
    }

    #[test]
    fn changing_commands_cannot_delete_a_previously_inspected_occurrence() {
        let mut app = App::new(
            vec![candidate(1, "alpha", 1), candidate(2, "beta", 0)],
            String::new(),
            1,
        );
        app.show_details(vec![Entry {
            id: 9,
            command_id: 1,
            command: "alpha".into(),
            started_at: 1,
            finished_at: None,
            duration_ms: None,
            exit_code: None,
            cwd: "/".into(),
            shell: "bash".into(),
            session_id: "s".into(),
            source: "live".into(),
        }]);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.selected_candidate().unwrap().id, 2);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.view, View::Manage);
        assert!(app.selected_detail().is_none());
    }

    #[test]
    fn wrong_confirmation_does_not_mutate() {
        let mut app = App::new(vec![candidate(1, "one", 0)], String::new(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.handle_key(key(KeyCode::Char('c')));
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('o')));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Intent::None);
    }

    #[test]
    fn renders_main_and_help_views_at_normal_size() {
        let mut app = App::new(vec![candidate(1, "git status", 1)], "git".into(), 1);
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let main = terminal.backend().buffer().content().to_vec();
        assert!(main.iter().any(|cell| cell.symbol() == "g"));

        app.handle_key(key(KeyCode::F(1)));
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let help = terminal.backend().buffer().content();
        assert!(
            help.windows(4)
                .any(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == "Help")
        );
    }

    #[test]
    fn narrow_terminal_render_does_not_panic() {
        let app = App::new(
            vec![candidate(1, "a very long command with many arguments", 1)],
            String::new(),
            1,
        );
        let backend = TestBackend::new(20, 7);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
    }

    #[test]
    fn long_history_renders_and_accepts_the_selected_result() {
        let candidates = (0..2000)
            .map(|id| candidate(id, &format!("command-{id:04}"), 0))
            .collect();
        let mut app = App::new(candidates, String::new(), 1);
        for _ in 0..1999 {
            app.handle_key(key(KeyCode::Down));
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("2000 matches"));
        assert!(text.contains("command-1999"));
        assert!(!text.contains("command-0000"));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::Accept("command-1999".into())
        );
    }

    #[test]
    fn navigation_and_empty_states_are_safe() {
        let mut empty = App::new(vec![], "x".into(), 1);
        assert_eq!(empty.handle_key(key(KeyCode::Enter)), Intent::Cancel);
        assert_eq!(empty.handle_key(key(KeyCode::F(2))), Intent::None);
        empty.handle_key(key(KeyCode::Backspace));
        assert!(empty.query.is_empty());

        let mut app = App::new(
            vec![candidate(1, "one", 1), candidate(2, "two", 1)],
            String::new(),
            1,
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        assert_eq!(app.selected, 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(app.selected, 0);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        app.handle_key(KeyCode::Char('T').into());
        assert_eq!(app.query, "T");
        assert_eq!(app.handle_key(key(KeyCode::Right)), Intent::None);
    }

    #[test]
    fn overlays_and_detail_management_cover_every_transition() {
        let mut app = App::new(vec![candidate(7, "one", 1)], String::new(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::CONTROL));
        assert_eq!(app.view, View::Help);
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(app.view, View::Main);

        app.show_details(vec![
            Entry {
                id: 10,
                command_id: 7,
                command: "one".into(),
                started_at: 1,
                finished_at: Some(2),
                duration_ms: Some(1),
                exit_code: Some(1),
                cwd: "/a".into(),
                shell: "fish".into(),
                session_id: "s".into(),
                source: "live".into(),
            },
            Entry {
                id: 11,
                command_id: 7,
                command: "one".into(),
                started_at: 2,
                finished_at: None,
                duration_ms: None,
                exit_code: None,
                cwd: "/b".into(),
                shell: "zsh".into(),
                session_id: "s".into(),
                source: "live".into(),
            },
        ]);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        assert_eq!(app.detail_selected, 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(app.detail_selected, 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(
            app.view,
            View::Confirm {
                mutation: Mutation::DeleteEntry(10),
                input: String::new()
            }
        );
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.view, View::Details);

        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(
            app.handle_key(key(KeyCode::Char('i'))),
            Intent::LoadDetails(7)
        );
        app.show_details(vec![]);
        assert_eq!(app.handle_key(key(KeyCode::Left)), Intent::None);
        assert_eq!(app.view, View::Main);
    }

    #[test]
    fn renders_details_manage_confirm_and_status_variants() {
        let mut failed = candidate(1, "failed\ncommand", 1);
        failed.latest_exit_code = Some(2);
        let mut unfinished = candidate(2, "unfinished", 1);
        unfinished.latest_exit_code = None;
        let mut app = App::new(vec![failed, unfinished], String::new(), 1);
        let backend = TestBackend::new(90, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();

        app.show_details(vec![Entry {
            id: 10,
            command_id: 1,
            command: "failed\ncommand".into(),
            started_at: 1,
            finished_at: Some(2),
            duration_ms: Some(1),
            exit_code: Some(2),
            cwd: "/tmp".into(),
            shell: "bash".into(),
            session_id: "s".into(),
            source: "live".into(),
        }]);
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        app.handle_key(key(KeyCode::Char('c')));
        terminal.draw(|frame| draw(frame, &app)).unwrap();
    }

    fn entry(id: i64, command_id: i64) -> Entry {
        Entry {
            id,
            command_id,
            command: "cargo test".into(),
            started_at: 1,
            finished_at: Some(1_801),
            duration_ms: Some(1_800),
            exit_code: Some(0),
            cwd: "/work/fmn".into(),
            shell: "zsh".into(),
            session_id: "seed".into(),
            source: "live".into(),
        }
    }

    #[test]
    fn live_preview_is_bounded_and_cannot_enable_occurrence_deletion() {
        let mut app = App::new(
            vec![candidate(1, "cargo test", 1), candidate(2, "git status", 1)],
            String::new(),
            1,
        );
        app.set_preview(1, (0..8).map(|id| entry(id, 1)).collect());
        assert_eq!(app.preview.len(), 3);
        assert_eq!(app.view, View::Main);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.view, View::Manage);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Down));
        app.set_preview(1, vec![entry(20, 1)]);
        assert_eq!(app.preview_command_id, Some(1));
        app.set_preview(2, vec![entry(21, 1), entry(22, 2)]);
        assert_eq!(app.preview_command_id, Some(2));
        assert_eq!(app.preview.len(), 1);
        assert_eq!(app.preview[0].id, 22);
    }

    #[test]
    fn search_cursor_edits_unicode_and_paste_cannot_activate_actions() {
        let mut app = App::new(vec![], "a👩‍💻e\u{301}中".into(), 1);
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.query, "a👩‍💻中");
        app.handle_key(key(KeyCode::Home));
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.query, "👩‍💻中");
        app.handle_key(key(KeyCode::End));
        app.handle_paste("\nfoo\x1b\r\tbar");
        assert_eq!(app.query, "👩‍💻中 foo  bar");
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert!(app.query.is_empty());
        assert_eq!(app.cursor, 0);
        app.open_overlay(View::Manage);
        app.handle_paste("clear");
        assert_eq!(app.view, View::Manage);
        assert!(app.query.is_empty());
    }

    #[test]
    fn actions_insert_and_theme_preview_can_be_cancelled_or_applied() {
        use crate::config::Theme;

        let mut app = App::new(vec![candidate(1, "cargo test", 1)], String::new(), 1);
        app.open_overlay(View::Manage);
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::Accept("cargo test".into())
        );
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.theme, Theme::Rose);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.theme, Theme::Mint);
        assert_eq!(app.view, View::Main);
        app.handle_key(key(KeyCode::F(3)));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.theme, Theme::Rose);
        assert_eq!(app.view, View::Main);
        app.handle_key(key(KeyCode::F(4)));
        assert!(!app.show_inspector);
    }

    #[test]
    fn mouse_uses_rendered_rows_and_overlays_isolate_the_background() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;

        let mut app = App::new(
            (0..30)
                .map(|id| candidate(id, &format!("command-{id:02}"), 1))
                .collect(),
            String::new(),
            1,
        );
        let area = Rect::new(0, 0, 120, 40);
        let layout = super::render::geometry(area, true);
        let (rows, height) = super::render::candidate_rows(layout.results);
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rows.x + 3,
            row: rows.y + height * 2 + 1,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(click, area);
        assert_eq!(app.selected, 2);
        app.open_overlay(View::Manage);
        app.handle_mouse(click, area);
        assert_eq!(app.selected, 2);
        let menu = super::render::menu_rows(area, &app);
        let result = app.handle_mouse(
            MouseEvent {
                column: menu.x,
                row: menu.y,
                ..click
            },
            area,
        );
        assert_eq!(result, Intent::Accept("command-02".into()));
    }

    #[test]
    fn every_overlay_renders_at_small_and_large_sizes_without_leaking_state() {
        for (width, height) in [
            (0, 0),
            (1, 1),
            (20, 7),
            (40, 12),
            (80, 24),
            (80, 40),
            (120, 40),
        ] {
            let mut app = App::new(
                vec![candidate(1, "printf 'hello 世界'\nnext\x1b[31m", 1)],
                "世".repeat(100),
                1,
            );
            app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for view in [
                View::Main,
                View::Details,
                View::Help,
                View::Manage,
                View::Themes,
                View::Confirm {
                    mutation: Mutation::Clear,
                    input: "clear".into(),
                },
            ] {
                app.view = view;
                terminal.draw(|frame| draw(frame, &app)).unwrap();
            }
        }
    }
}
