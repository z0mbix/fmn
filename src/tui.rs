use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::{Config, Theme, UiConfig, UiPreference};
use crate::keybindings::{KeyAction, KeyBinding, KeyBindings};
use crate::model::{Entry, HistoryStats, SearchCandidate};
use crate::scope::{SearchContext, SearchScope};
use crate::search::{RankedMatch, Ranker};
use crate::storage::Store;
use crate::{Error, Result};

mod bindings;
mod dashboard;
mod render;
mod terminal;
mod theme;

#[cfg(test)]
mod features_tests;

use render::draw;
use terminal::TuiTerminal;
use theme::{THEMES, ThemePicker};

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
    SaveTheme(Theme),
    SavePreference(UiPreference),
    ReloadScope,
    LoadStats,
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
    Stats,
    Settings,
    Keybindings,
    Confirm { mutation: Mutation, input: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Insert,
    Inspect,
    Theme,
    Delete,
    Forget,
    Stats,
    Settings,
}

#[derive(Debug)]
pub struct App {
    query: String,
    cursor: usize,
    candidates: Vec<SearchCandidate>,
    ranked: Vec<RankedMatch>,
    ranker: Ranker,
    selected: usize,
    scope: SearchScope,
    context: SearchContext,
    preferences: UiConfig,
    shell_integration: bool,
    stats: Option<HistoryStats>,
    stats_scroll: u16,
    area: Rect,
    setting_selected: usize,
    binding_selected: usize,
    binding_pending: Option<KeyAction>,
    binding_error: Option<String>,
    return_to_settings: bool,
    view: View,
    details: Vec<Entry>,
    detail_selected: usize,
    preview: Vec<Entry>,
    preview_command_id: Option<i64>,
    return_to_details: bool,
    action_selected: usize,
    theme: Theme,
    previous_theme: Theme,
    theme_picker: ThemePicker,
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
            scope: SearchScope::Global,
            context: SearchContext {
                cwd: String::new(),
                repository: None,
                session: None,
            },
            preferences: UiConfig::default(),
            shell_integration: true,
            stats: None,
            stats_scroll: 0,
            area: Rect::new(0, 0, 80, 24),
            setting_selected: 0,
            binding_selected: 0,
            binding_pending: None,
            binding_error: None,
            return_to_settings: false,
            view: View::Main,
            details: Vec::new(),
            detail_selected: 0,
            preview: Vec::new(),
            preview_command_id: None,
            return_to_details: false,
            action_selected: 0,
            theme: Theme::default(),
            previous_theme: Theme::default(),
            theme_picker: ThemePicker::default(),
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
        self.return_to_settings =
            self.view == View::Settings && matches!(view, View::Themes | View::Keybindings);
        self.action_selected = 0;
        if view == View::Stats {
            self.stats_scroll = 0;
        }
        if view == View::Settings {
            self.setting_selected = 0;
        }
        if view == View::Keybindings {
            self.binding_selected = 0;
            self.binding_pending = None;
            self.binding_error = None;
        }
        if view == View::Themes {
            self.previous_theme = self.theme;
            self.theme_picker.query.clear();
            self.theme_picker.refilter();
        }
        self.view = view;
    }

    fn close_overlay(&mut self) {
        if self.return_to_settings {
            self.view = View::Settings;
            self.return_to_settings = false;
            return;
        }
        self.view = if self.return_to_details {
            View::Details
        } else {
            View::Main
        };
        self.return_to_details = false;
    }

    fn theme_index(&self) -> usize {
        self.theme_picker
            .matches
            .iter()
            .position(|matched| THEMES[matched.index].0 == self.theme)
            .unwrap_or(0)
    }

    fn confirm_theme(&mut self) -> Intent {
        if self.theme_picker.matches.is_empty() {
            return Intent::None;
        }
        self.preferences.theme = self.theme;
        self.close_overlay();
        Intent::SaveTheme(self.theme)
    }

    fn preview_theme(&mut self, index: usize) {
        if let Some(matched) = self.theme_picker.matches.get(index) {
            self.theme = THEMES[matched.index].0;
        }
    }

    fn refilter_themes(&mut self) {
        self.theme_picker.refilter();
        if !self.theme_picker.query.trim().is_empty() {
            self.preview_theme(0);
        }
    }

    fn handle_theme_key(&mut self, key: KeyEvent) -> Intent {
        let index = self.theme_index();
        let last = self.theme_picker.matches.len().saturating_sub(1);
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.theme = self.previous_theme;
                self.close_overlay();
            }
            (KeyCode::Enter, _) => return self.confirm_theme(),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.preview_theme(index.saturating_sub(1));
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.preview_theme((index + 1).min(last));
            }
            (KeyCode::Home, _) => self.preview_theme(0),
            (KeyCode::End, _) => self.preview_theme(last),
            (KeyCode::PageUp, _) => self.preview_theme(index.saturating_sub(10)),
            (KeyCode::PageDown, _) => self.preview_theme((index + 10).min(last)),
            (KeyCode::Backspace, _) => {
                if let Some((start, _)) = self.theme_picker.query.grapheme_indices(true).next_back()
                {
                    self.theme_picker.query.truncate(start);
                    self.refilter_themes();
                }
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.theme_picker.query.clear();
                self.refilter_themes();
            }
            (KeyCode::Char(character), modifiers)
                if !character.is_control()
                    && (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT) =>
            {
                self.theme_picker.query.push(character);
                self.refilter_themes();
            }
            _ => {}
        }
        Intent::None
    }

    fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::Insert, Action::Inspect, Action::Theme];
        if self.selected_detail().is_some() {
            actions.push(Action::Delete);
        }
        actions.extend([Action::Stats, Action::Settings, Action::Forget]);
        actions
    }

    fn cycle_scope(&mut self, backwards: bool) -> Intent {
        self.scope = self.context.next(self.scope, backwards);
        self.return_to_details = false;
        self.selected = 0;
        self.refresh();
        Intent::ReloadScope
    }

    fn change_preference(&mut self, preference: UiPreference) -> Intent {
        match preference {
            UiPreference::Theme(theme) => {
                self.preferences.theme = theme;
                self.theme = theme;
            }
            UiPreference::ShowInspector(show) => {
                self.preferences.show_inspector = show;
                self.show_inspector = show;
            }
            UiPreference::DefaultScope(scope) => {
                self.preferences.default_scope = scope;
                self.scope = self.context.effective(scope);
                self.selected = 0;
                self.return_to_details = false;
            }
            UiPreference::ExecuteOnSelect(execute) => {
                self.preferences.execute_on_select = execute;
                self.execute_on_select = execute && self.shell_integration;
            }
            UiPreference::Keybinding(action, binding) => {
                if let Err(error) = self.preferences.keybindings.rebind(action, binding) {
                    self.binding_error = Some(error);
                    return Intent::None;
                }
            }
            UiPreference::ResetKeybindings => self.preferences.keybindings = KeyBindings::default(),
        }
        Intent::SavePreference(preference)
    }

    fn activate_setting(&mut self, backwards: bool) -> Intent {
        let preference = match self.setting_selected {
            0 => return self.activate_action(Action::Theme),
            1 => UiPreference::ShowInspector(!self.show_inspector),
            2 => {
                let index = SearchScope::ALL
                    .iter()
                    .position(|scope| *scope == self.preferences.default_scope)
                    .unwrap_or(0);
                let index = (index + if backwards { 3 } else { 1 }) % 4;
                UiPreference::DefaultScope(SearchScope::ALL[index])
            }
            3 => UiPreference::ExecuteOnSelect(!self.preferences.execute_on_select),
            _ => {
                self.open_overlay(View::Keybindings);
                return Intent::None;
            }
        };
        self.change_preference(preference)
    }

    fn activate_shortcut(&mut self, action: KeyAction) -> Intent {
        match action {
            KeyAction::Inspector => {
                self.change_preference(UiPreference::ShowInspector(!self.show_inspector))
            }
            KeyAction::Themes => self.activate_action(Action::Theme),
            KeyAction::Details => self.activate_action(Action::Inspect),
            KeyAction::Statistics => self.activate_action(Action::Stats),
            KeyAction::Settings => self.activate_action(Action::Settings),
            KeyAction::Scope => self.cycle_scope(false),
            KeyAction::Actions => {
                self.open_overlay(View::Manage);
                Intent::None
            }
            KeyAction::Help => {
                self.open_overlay(View::Help);
                Intent::None
            }
        }
    }

    fn handle_binding_key(&mut self, key: KeyEvent) -> Intent {
        if let Some(action) = self.binding_pending {
            if key.code == KeyCode::Esc {
                self.binding_pending = None;
                self.binding_error = None;
                return Intent::None;
            }
            match KeyBinding::from_event(key) {
                Ok(binding) => {
                    let intent = self.change_preference(UiPreference::Keybinding(action, binding));
                    if intent != Intent::None {
                        self.binding_pending = None;
                        self.binding_error = None;
                    }
                    return intent;
                }
                Err(error) => self.binding_error = Some(error),
            }
            return Intent::None;
        }
        self.binding_error = None;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => self.close_overlay(),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.binding_selected = self.binding_selected.saturating_sub(1)
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.binding_selected = (self.binding_selected + 1).min(KeyAction::ALL.len() - 1)
            }
            (KeyCode::Home, _) => self.binding_selected = 0,
            (KeyCode::End, _) => self.binding_selected = KeyAction::ALL.len() - 1,
            (KeyCode::Enter, _) => {
                self.binding_pending = Some(KeyAction::ALL[self.binding_selected])
            }
            (KeyCode::Backspace | KeyCode::Delete, _) => {
                let action = KeyAction::ALL[self.binding_selected];
                return self
                    .change_preference(UiPreference::Keybinding(action, action.default_binding()));
            }
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                return self.change_preference(UiPreference::ResetKeybindings);
            }
            _ => {}
        }
        Intent::None
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> Intent {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => self.close_overlay(),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.setting_selected = self.setting_selected.saturating_sub(1)
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.setting_selected = (self.setting_selected + 1).min(4)
            }
            (KeyCode::Home, _) => self.setting_selected = 0,
            (KeyCode::End, _) => self.setting_selected = 4,
            (KeyCode::Enter | KeyCode::Right | KeyCode::Left | KeyCode::Char(' '), _) => {
                return self.activate_setting(key.code == KeyCode::Left);
            }
            _ => {}
        }
        Intent::None
    }

    fn handle_stats_key(&mut self, key: KeyEvent) -> Intent {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => self.close_overlay(),
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.stats_scroll = self.stats_scroll.saturating_sub(1)
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.stats_scroll = self
                    .stats_scroll
                    .saturating_add(1)
                    .min(dashboard::stats_max_scroll(self))
            }
            (KeyCode::PageUp, _) => self.stats_scroll = self.stats_scroll.saturating_sub(8),
            (KeyCode::PageDown, _) => {
                self.stats_scroll = self
                    .stats_scroll
                    .saturating_add(8)
                    .min(dashboard::stats_max_scroll(self))
            }
            (KeyCode::Home, _) => self.stats_scroll = 0,
            (KeyCode::End, _) => self.stats_scroll = dashboard::stats_max_scroll(self),
            (KeyCode::BackTab, _) => return self.cycle_scope(true),
            (KeyCode::Char('r'), _) => return Intent::LoadStats,
            _ => {}
        }
        Intent::None
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
            Action::Stats => {
                self.open_overlay(View::Stats);
                Intent::LoadStats
            }
            Action::Settings => {
                self.open_overlay(View::Settings);
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

        if matches!(self.view, View::Main | View::Details)
            && let Some(action) = self.preferences.keybindings.action(key)
        {
            return self.activate_shortcut(action);
        }
        if self.view == View::Stats
            && self
                .preferences
                .keybindings
                .get(KeyAction::Scope)
                .matches(key)
        {
            return self.cycle_scope(false);
        }
        match &mut self.view {
            View::Main => self.handle_main(key),
            View::Details => self.handle_details(key),
            View::Help | View::Manage => self.handle_overlay(key),
            View::Themes => self.handle_theme_key(key),
            View::Stats => self.handle_stats_key(key),
            View::Settings => self.handle_settings_key(key),
            View::Keybindings => self.handle_binding_key(key),
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
            (KeyCode::BackTab, _) => self.cycle_scope(true),
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
            KeyCode::Char('i') => return self.activate_action(Action::Inspect),
            KeyCode::Char('t') => return self.activate_action(Action::Theme),
            KeyCode::Char('d') => return self.activate_action(Action::Delete),
            KeyCode::Char('f') => return self.activate_action(Action::Forget),
            KeyCode::Char('g') => return self.activate_action(Action::Stats),
            KeyCode::Char('s') => return self.activate_action(Action::Settings),
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
            self.scope == SearchScope::Directory,
            self.now_millis,
            &mut self.ranked,
        );
        self.selected = self.selected.min(self.ranked.len().saturating_sub(1));
    }

    fn handle_paste(&mut self, text: &str) {
        if matches!(self.view, View::Main | View::Themes) {
            let text: String = text
                .chars()
                .map(|c| if c.is_whitespace() { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            if self.view == View::Themes {
                self.theme_picker.query.push_str(&text);
                self.refilter_themes();
            } else {
                self.query.insert_str(self.cursor, &text);
                self.cursor += text.len();
                self.refresh();
            }
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Intent {
        if self.view == View::Keybindings {
            return bindings::handle_mouse(self, mouse, area);
        }
        let position = Position::new(mouse.column, mouse.row);
        if matches!(self.view, View::Stats | View::Settings) {
            return dashboard::handle_mouse(self, mouse, area);
        }
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
            View::Themes => {
                let rows = render::menu_rows(area, self);
                if rows.contains(position) {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            self.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                        }
                        MouseEventKind::ScrollDown => {
                            self.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            let range = render::theme_range(area, self);
                            let index = range.start + usize::from(mouse.row - rows.y);
                            if index < range.end {
                                self.preview_theme(index);
                                return self.confirm_theme();
                            }
                        }
                        _ => {}
                    }
                }
            }
            View::Manage if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let rows = render::menu_rows(area, self);
                if rows.contains(position) {
                    let visible = render::action_range(area, self);
                    let index = visible.start + usize::from(mouse.row - rows.y);
                    if index < visible.end {
                        self.action_selected = index;
                        return self.activate_action(self.actions()[index]);
                    }
                }
            }
            _ => {}
        }
        Intent::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub command: String,
    pub execute: bool,
}

pub fn run(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
    config_path: &Path,
) -> Result<Option<Selection>> {
    if !io::stderr().is_terminal() || !io::stdin().is_terminal() {
        return Err(Error::Io(io::Error::other(
            "search requires an interactive terminal",
        )));
    }

    run_with_backend(store, query, cwd, config, config_path, false, io::stderr())
}

pub fn run_for_shell(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
    config_path: &Path,
) -> Result<Option<Selection>> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return Err(Error::Io(io::Error::other(
            "search requires an interactive terminal",
        )));
    }
    run_with_backend(store, query, cwd, config, config_path, true, io::stdout())
}

fn run_with_backend<W: Write>(
    store: &mut Store,
    query: String,
    cwd: String,
    config: &UiConfig,
    config_path: &Path,
    shell_integration: bool,
    writer: W,
) -> Result<Option<Selection>> {
    let context = SearchContext::detect(cwd.clone());
    let scope = context.effective(config.default_scope);
    let candidates = store.scoped_search_candidates(&context, scope)?;
    let mut app = App::new(candidates, query, now_millis());
    app.context = context;
    app.scope = scope;
    app.preferences = config.clone();
    app.shell_integration = shell_integration;
    app.theme = if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        Theme::NoColor
    } else {
        config.theme
    };
    app.show_inspector = config.show_inspector;
    app.execute_on_select = config.execute_on_select && shell_integration;
    app.current_dir = cwd.clone();
    let mut terminal = TuiTerminal::enter(writer)?;

    loop {
        if terminal.interrupted() {
            return Ok(None);
        }
        if let Some(command_id) = app.selected_candidate().map(|candidate| candidate.id)
            && app.preview_command_id != Some(command_id)
        {
            app.set_preview(
                command_id,
                store.scoped_entries_for_command(command_id, 3, &app.context, app.scope)?,
            );
        }
        let size = terminal.terminal.size()?;
        app.area = Rect::new(0, 0, size.width, size.height);
        app.stats_scroll = app.stats_scroll.min(dashboard::stats_max_scroll(&app));
        terminal.terminal.draw(|frame| draw(frame, &app))?;
        // A bounded wait lets shutdown signals reach the terminal guard even
        // when the user is not pressing keys.
        while !event::poll(Duration::from_millis(50))? {
            if terminal.interrupted() {
                return Ok(None);
            }
        }
        if terminal.interrupted() {
            return Ok(None);
        }
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
            Intent::Accept(command) => {
                return Ok(Some(Selection {
                    command,
                    execute: app.execute_on_select,
                }));
            }
            Intent::Cancel => return Ok(None),
            Intent::SaveTheme(theme) => Config::save_theme(config_path, theme)?,
            Intent::SavePreference(preference) => {
                Config::save_preference(config_path, preference)?;
                if matches!(preference, UiPreference::DefaultScope(_)) {
                    app.set_candidates(store.scoped_search_candidates(&app.context, app.scope)?);
                }
            }
            Intent::ReloadScope => {
                app.set_candidates(store.scoped_search_candidates(&app.context, app.scope)?);
                if app.view == View::Stats {
                    app.stats = Some(store.history_stats(&app.context, app.scope, now_millis())?);
                    app.stats_scroll = 0;
                }
            }
            Intent::LoadStats => {
                app.stats = Some(store.history_stats(&app.context, app.scope, now_millis())?);
                app.stats_scroll = 0;
            }
            Intent::LoadDetails(command_id) => {
                app.show_details(store.scoped_entries_for_command(
                    command_id,
                    -1,
                    &app.context,
                    app.scope,
                )?);
            }
            Intent::Mutate(mutation) => {
                match mutation {
                    Mutation::DeleteEntry(entry_id) => store.delete_entry(entry_id)?,
                    Mutation::ForgetCommand(command_id) => store.forget_command(command_id)?,
                    Mutation::Clear => store.clear()?,
                }
                app.set_candidates(store.scoped_search_candidates(&app.context, app.scope)?);
            }
        }
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

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
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

        app.handle_key(key(KeyCode::Char('?')));
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
    fn scope_shortcut_toggles_strict_directory_filter() {
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
        assert_eq!(app.handle_key(ctrl('d')), Intent::LoadDetails(1));
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
        app.handle_key(key(KeyCode::Char('f')));
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

        app.handle_key(key(KeyCode::Char('?')));
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
        assert_eq!(empty.handle_key(ctrl('d')), Intent::None);
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
        app.handle_key(key(KeyCode::Char('?')));
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
        assert_eq!(app.handle_key(key(KeyCode::Down)), Intent::None);
        assert_eq!(app.theme, Theme::Rose);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Intent::None);
        assert_eq!(app.theme, Theme::Mint);
        assert_eq!(app.view, View::Main);
        app.handle_key(ctrl('t'));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::SaveTheme(Theme::Rose)
        );
        assert_eq!(app.theme, Theme::Rose);
        assert_eq!(app.view, View::Main);
        app.handle_key(ctrl('o'));
        assert!(!app.show_inspector);
    }

    #[test]
    fn theme_filter_is_separate_from_history_and_cancel_restores_the_previous_view() {
        use super::THEMES;
        use crate::config::Theme;

        let mut app = App::new(vec![candidate(1, "cargo test", 1)], "cargo".into(), 1);
        app.show_details(vec![entry(1, 1)]);
        app.handle_key(ctrl('t'));
        for character in "RP-D".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        assert_eq!(app.theme, Theme::RosePineDawn);
        assert_eq!(app.theme_picker.matches.len(), 1);
        assert_eq!(app.query, "cargo");
        assert_eq!(app.details.len(), 1);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.theme, Theme::Mint);
        assert_eq!(app.view, View::Details);
        app.handle_key(ctrl('t'));
        assert!(app.theme_picker.query.is_empty());
        assert_eq!(app.theme_picker.matches.len(), THEMES.len());
    }

    #[test]
    fn theme_filter_edits_graphemes_sanitizes_paste_and_cannot_confirm_an_empty_list() {
        use crate::config::Theme;

        let mut app = App::new(vec![], "history".into(), 1);
        app.handle_key(ctrl('t'));
        app.handle_paste("nord👩‍💻e\u{301}");
        assert!(app.theme_picker.matches.is_empty());
        for code in [
            KeyCode::Enter,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
        ] {
            assert_eq!(app.handle_key(key(code)), Intent::None);
            assert_eq!(app.view, View::Themes);
        }
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.theme_picker.query, "nord👩‍💻");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.theme_picker.query, "nord");
        assert_eq!(app.theme, Theme::Nord);
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        app.handle_paste("CAT\nmocha\x1b\r");
        assert_eq!(app.theme_picker.query, "CAT mocha ");
        assert_eq!(app.view, View::Themes);
        assert_eq!(app.theme, Theme::CatppuccinMocha);
        assert_eq!(app.query, "history");
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Intent::SaveTheme(Theme::CatppuccinMocha)
        );
    }

    #[test]
    fn filtered_theme_mouse_selection_tracks_scrolled_rows_after_resize() {
        use super::{THEMES, render};
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;

        let mut app = App::new(vec![], String::new(), 1);
        for area in [Rect::new(0, 0, 80, 24), Rect::new(0, 0, 40, 12)] {
            app.handle_key(ctrl('t'));
            app.handle_paste("a");
            app.handle_key(key(KeyCode::End));
            let visible = render::theme_range(area, &app);
            assert!(visible.start > 0);
            let expected = THEMES[app.theme_picker.matches[visible.start].index].0;
            let rows = render::menu_rows(area, &app);
            let intent = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rows.x,
                    row: rows.y,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
            assert_eq!(intent, Intent::SaveTheme(expected));
            assert_eq!(app.theme, expected);
            assert_eq!(app.view, View::Main);
        }
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
    fn theme_picker_scrolls_and_mouse_hits_the_visible_theme_after_resize() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;

        use super::{THEMES, render};
        use crate::config::Theme;

        let mut app = App::new(vec![], String::new(), 1);
        app.handle_key(ctrl('t'));
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.theme, Theme::EverforestLightSoft);
        for area in [Rect::new(0, 0, 80, 24), Rect::new(0, 0, 40, 12)] {
            let rows = render::menu_rows(area, &app);
            let visible = render::theme_range(area, &app);
            assert!(visible.start > 0);
            assert!(visible.contains(&app.theme_index()));
            let expected = THEMES[visible.start].0;
            let intent = app.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rows.x,
                    row: rows.y,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
            assert_eq!(intent, Intent::SaveTheme(expected));
            assert_eq!(app.theme, expected);
            assert_eq!(app.view, View::Main);
            app.handle_key(ctrl('t'));
            app.handle_key(key(KeyCode::End));
        }
        let expected_on_cancel = app.previous_theme;
        let area = Rect::new(0, 0, 80, 24);
        let rows = render::menu_rows(area, &app);
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: rows.x,
            row: rows.y,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(mouse, area);
        assert_eq!(app.theme, Theme::EverforestLightMedium);
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..mouse
            },
            area,
        );
        assert_eq!(app.theme, Theme::EverforestLightSoft);
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.theme, Theme::Mint);
        app.handle_key(key(KeyCode::PageDown));
        assert_eq!(app.theme_index(), 10);
        app.handle_key(key(KeyCode::PageUp));
        assert_eq!(app.theme, Theme::Mint);
        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.theme, expected_on_cancel);
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
