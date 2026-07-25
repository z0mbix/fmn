use std::io::{self, IsTerminal};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::{Entry, SearchCandidate};
use crate::search::{RankedCandidate, rank};
use crate::storage::Store;
use crate::{Error, Result};

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
    Confirm { mutation: Mutation, input: String },
}

#[derive(Debug)]
pub struct App {
    query: String,
    candidates: Vec<SearchCandidate>,
    ranked: Vec<RankedCandidate>,
    selected: usize,
    current_dir_only: bool,
    view: View,
    details: Vec<Entry>,
    detail_selected: usize,
    now_millis: i64,
}

impl App {
    pub fn new(candidates: Vec<SearchCandidate>, query: String, now_millis: i64) -> Self {
        let ranked = rank(&candidates, &query, false, now_millis);
        Self {
            query,
            candidates,
            ranked,
            selected: 0,
            current_dir_only: false,
            view: View::Main,
            details: Vec::new(),
            detail_selected: 0,
            now_millis,
        }
    }

    pub fn set_candidates(&mut self, candidates: Vec<SearchCandidate>) {
        self.candidates = candidates;
        self.refresh();
    }

    pub fn show_details(&mut self, entries: Vec<Entry>) {
        self.details = entries;
        self.detail_selected = 0;
        self.view = View::Details;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Intent {
        match &mut self.view {
            View::Main => self.handle_main(key),
            View::Details => self.handle_details(key),
            View::Help | View::Manage => self.handle_overlay(key),
            View::Confirm { mutation, input } => {
                let mutation = mutation.clone();
                match key.code {
                    KeyCode::Esc => self.view = View::Main,
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
                self.view = View::Help;
                Intent::None
            }
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                self.view = View::Manage;
                Intent::None
            }
            (KeyCode::Backspace, _) => {
                self.query.pop();
                self.refresh();
                Intent::None
            }
            (KeyCode::Char(character), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                self.query.push(character);
                self.refresh();
                Intent::None
            }
            _ => Intent::None,
        }
    }

    fn handle_details(&mut self, key: KeyEvent) -> Intent {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Left, _) => {
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
                self.view = View::Manage;
                Intent::None
            }
            _ => Intent::None,
        }
    }

    fn handle_overlay(&mut self, key: KeyEvent) -> Intent {
        if self.view == View::Help {
            self.view = View::Main;
            return Intent::None;
        }
        match key.code {
            KeyCode::Esc => self.view = View::Main,
            KeyCode::Char('i') | KeyCode::Enter => {
                if let Some(command_id) = self.selected_candidate().map(|candidate| candidate.id) {
                    self.view = View::Main;
                    return Intent::LoadDetails(command_id);
                }
            }
            KeyCode::Char('d') => {
                if let Some(entry) = self.details.get(self.detail_selected) {
                    self.view = View::Confirm {
                        mutation: Mutation::DeleteEntry(entry.id),
                        input: String::new(),
                    };
                }
            }
            KeyCode::Char('f') => {
                if let Some(command_id) = self.selected_candidate().map(|candidate| candidate.id) {
                    self.view = View::Confirm {
                        mutation: Mutation::ForgetCommand(command_id),
                        input: String::new(),
                    };
                }
            }
            KeyCode::Char('c') => {
                self.view = View::Confirm {
                    mutation: Mutation::Clear,
                    input: String::new(),
                };
            }
            _ => {}
        }
        Intent::None
    }

    fn selected_candidate(&self) -> Option<&SearchCandidate> {
        self.ranked
            .get(self.selected)
            .map(|ranked| &ranked.candidate)
    }

    fn refresh(&mut self) {
        self.ranked = rank(
            &self.candidates,
            &self.query,
            self.current_dir_only,
            self.now_millis,
        );
        self.selected = self.selected.min(self.ranked.len().saturating_sub(1));
    }
}

pub fn run(store: &mut Store, query: String, cwd: String) -> Result<Option<String>> {
    if !io::stderr().is_terminal() || !io::stdin().is_terminal() {
        return Err(Error::Io(io::Error::other(
            "search requires an interactive terminal",
        )));
    }

    let candidates = store.search_candidates(&cwd)?;
    let mut app = App::new(candidates, query, now_millis());
    let mut terminal = TuiTerminal::enter()?;

    loop {
        terminal.terminal.draw(|frame| draw(frame, &app))?;
        if let Event::Key(key) = event::read()? {
            if !key.is_press() {
                continue;
            }
            match app.handle_key(key) {
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
}

struct TuiTerminal {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
}

impl TuiTerminal {
    fn enter() -> Result<Self> {
        enable_raw_mode().map_err(|error| {
            Error::Io(io::Error::new(
                error.kind(),
                format!("could not enable terminal raw mode: {error}"),
            ))
        })?;
        let mut stderr = io::stderr();
        if let Err(error) = execute!(stderr, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(Error::Io(io::Error::new(
                error.kind(),
                format!("could not enter alternate screen: {error}"),
            )));
        }
        let terminal = Terminal::new(CrosstermBackend::new(stderr)).map_err(|error| {
            Error::Io(io::Error::new(
                error.kind(),
                format!("could not initialize terminal: {error}"),
            ))
        })?;
        Ok(Self { terminal })
    }
}

impl Drop for TuiTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let scope = if app.current_dir_only {
        "current directory"
    } else {
        "global"
    };
    let input = Paragraph::new(app.query.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" fmn history · {scope} ")),
        )
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(input, areas[0]);

    match &app.view {
        View::Details => draw_details(frame, areas[1], app),
        View::Help => draw_help(frame, areas[1]),
        View::Manage => draw_manage(frame, areas[1], app),
        View::Confirm { mutation, input } => {
            let expected = mutation_confirmation(mutation);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(format!("This action is permanent: {mutation:?}")),
                    Line::from(format!("Type {expected} and press Enter: {input}")),
                    Line::from("Escape cancels."),
                ])
                .block(Block::default().borders(Borders::ALL).title(" Confirm "))
                .wrap(Wrap { trim: false }),
                areas[1],
            );
        }
        View::Main => draw_candidates(frame, areas[1], app),
    }

    frame.render_widget(
        Line::from(vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" insert  "),
            Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" scope  "),
            Span::styled("F2", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" details  "),
            Span::styled("Ctrl-X", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" manage  "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" cancel"),
        ]),
        areas[2],
    );
}

fn draw_candidates(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let items: Vec<_> = app
        .ranked
        .iter()
        .map(|ranked| {
            let status = match ranked.candidate.latest_exit_code {
                Some(0) => "✓",
                Some(_) => "✗",
                None => "·",
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{status} "), Style::default().fg(Color::DarkGray)),
                Span::raw(ranked.candidate.command.replace('\n', " ↵ ")),
                Span::styled(
                    format!(
                        "  ×{}  {}",
                        ranked.candidate.use_count, ranked.candidate.latest_cwd
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected((!items.is_empty()).then_some(app.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} matches ", app.ranked.len())),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_details(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let items: Vec<_> = app
        .details
        .iter()
        .map(|entry| {
            ListItem::new(format!(
                "{}  {}  exit={}  {}",
                entry.started_at,
                entry.cwd,
                entry
                    .exit_code
                    .map_or_else(|| "?".into(), |code| code.to_string()),
                entry.shell
            ))
        })
        .collect();
    let mut state =
        ListState::default().with_selected((!items.is_empty()).then_some(app.detail_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Occurrences "),
            )
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_help(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect) {
    frame.render_widget(
        Paragraph::new(
            "Type to fuzzy-search. Enter inserts without executing. Tab toggles directory scope.\n\
             F2 shows occurrences. Ctrl-X opens management. Escape cancels.",
        )
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_manage(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let delete = if app.details.is_empty() {
        ""
    } else {
        "d delete selected occurrence\n"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "i inspect occurrences\n{delete}f forget selected command everywhere\nc clear all history\nEsc cancel"
        ))
        .block(Block::default().borders(Borders::ALL).title(" Manage ")),
        area,
    );
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
    fn tab_toggles_strict_directory_filter() {
        let mut app = App::new(
            vec![candidate(1, "global", 0), candidate(2, "local", 1)],
            String::new(),
            1,
        );
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.ranked.len(), 1);
        assert_eq!(app.ranked[0].candidate.command, "local");
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
        assert_eq!(app.view, View::Main);

        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Intent::LoadDetails(7));
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
}
