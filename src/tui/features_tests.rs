use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn ctrl(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn app() -> App {
    let mut app = App::new(vec![], "cargo".into(), 1);
    app.context = SearchContext {
        cwd: "/repo/src".into(),
        repository: Some("/repo".into()),
        session: Some("current".into()),
    };
    app.stats = Some(HistoryStats {
        total: 10,
        unique: 2,
        completed: 8,
        failed: 2,
        average_duration_ms: Some(1_000.0),
        activity: [1; 30],
        top_commands: vec![
            crate::model::CommandStats {
                command: "cargo test\n--all\x1b[31m".into(),
                count: 10,
                failures: 2,
                average_duration_ms: Some(1_000.0)
            };
            5
        ],
        ..HistoryStats::default()
    });
    app
}

#[test]
fn scopes_cycle_both_directions_without_changing_the_query_or_persisting_temporary_choices() {
    let mut app = app();
    for expected in [
        SearchScope::Directory,
        SearchScope::Repository,
        SearchScope::Session,
        SearchScope::Global,
    ] {
        assert_eq!(app.handle_key(key(KeyCode::Tab)), Intent::ReloadScope);
        assert_eq!(app.scope, expected);
        assert_eq!(app.query, "cargo");
        assert_eq!(app.preferences.default_scope, SearchScope::Global);
    }
    assert_eq!(app.handle_key(key(KeyCode::BackTab)), Intent::ReloadScope);
    assert_eq!(app.scope, SearchScope::Session);
}

#[test]
fn settings_apply_one_preference_at_a_time_and_nested_themes_return_to_settings() {
    let mut app = app();
    app.view = View::Details;
    app.handle_key(ctrl('s'));
    assert_eq!(app.view, View::Settings);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Themes);
    app.handle_paste("nord");
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        Intent::SaveTheme(Theme::Nord)
    );
    assert_eq!(app.view, View::Settings);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        Intent::SavePreference(UiPreference::ShowInspector(false))
    );
    assert!(!app.show_inspector);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(
        app.handle_key(key(KeyCode::Left)),
        Intent::SavePreference(UiPreference::DefaultScope(SearchScope::Session))
    );
    assert_eq!(app.scope, SearchScope::Session);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        Intent::SavePreference(UiPreference::ExecuteOnSelect(true))
    );
    assert!(app.execute_on_select);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.handle_key(key(KeyCode::Enter)), Intent::None);
    assert_eq!(app.view, View::Keybindings);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.view, View::Settings);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.view, View::Main);
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(app.view, View::Settings);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
        Intent::LoadStats
    );
    assert_eq!(app.view, View::Stats);
}

#[test]
fn cancelled_theme_from_settings_restores_the_theme_and_previous_detail_view() {
    let mut app = app();
    app.view = View::Details;
    app.handle_key(ctrl('s'));
    app.handle_key(key(KeyCode::Enter));
    app.handle_paste("dracula");
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.view, View::Settings);
    assert_eq!(app.theme, Theme::Mint);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.view, View::Details);
}

#[test]
fn standalone_settings_cannot_enable_execution_and_unavailable_defaults_fall_back_to_global() {
    let mut app = app();
    app.shell_integration = false;
    app.change_preference(UiPreference::ExecuteOnSelect(true));
    assert!(app.preferences.execute_on_select);
    assert!(!app.execute_on_select);
    app.context.repository = None;
    app.change_preference(UiPreference::DefaultScope(SearchScope::Repository));
    assert_eq!(app.scope, SearchScope::Global);
    assert_eq!(app.preferences.default_scope, SearchScope::Repository);
}

#[test]
fn statistics_scroll_refresh_and_scope_changes_leave_command_selection_untouched() {
    let mut app = app();
    assert_eq!(app.handle_key(ctrl('g')), Intent::LoadStats);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.stats_scroll, dashboard::stats_max_scroll(&app));
    app.handle_key(key(KeyCode::Up));
    assert_eq!(
        app.stats_scroll,
        dashboard::stats_max_scroll(&app).saturating_sub(1)
    );
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.stats_scroll, 0);
    assert_eq!(app.handle_key(key(KeyCode::Char('r'))), Intent::LoadStats);
    assert_eq!(app.handle_key(key(KeyCode::Tab)), Intent::ReloadScope);
    assert_eq!(app.scope, SearchScope::Directory);
    assert_eq!(app.query, "cargo");
    assert_eq!(app.view, View::Stats);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Intent::Cancel
    );
}

#[test]
fn menus_never_offer_clear_and_statistics_and_settings_render_in_every_theme_and_size() {
    for (theme, _) in THEMES {
        for (width, height) in [(0, 0), (1, 1), (20, 7), (40, 12), (80, 24), (120, 40)] {
            let mut app = app();
            app.theme = theme;
            app.area = Rect::new(0, 0, width, height);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for view in [View::Stats, View::Settings, View::Keybindings, View::Manage] {
                app.view = view;
                terminal.draw(|frame| draw(frame, &app)).unwrap();
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                assert!(!text.contains("Clear all history"));
                if width >= 80 && height >= 24 {
                    let expected = match app.view {
                        View::Stats => "25.0% failed",
                        View::Settings => "Changes save automatically",
                        View::Keybindings => "Select an action",
                        _ => "History statistics",
                    };
                    assert!(text.contains(expected), "{expected} missing");
                }
                if theme == Theme::NoColor {
                    for cell in terminal.backend().buffer().content() {
                        assert_eq!(cell.fg, ratatui::style::Color::Reset);
                        assert_eq!(cell.bg, ratatui::style::Color::Reset);
                    }
                }
            }
            app.view = View::Manage;
            app.handle_key(key(KeyCode::Char('c')));
            assert_eq!(app.view, View::Manage);
        }
    }
}

#[test]
fn tab_cycles_scopes_ctrl_o_toggles_inspector_and_question_opens_help() {
    let mut app = app();
    assert_eq!(
        app.handle_key(ctrl('o')),
        Intent::SavePreference(UiPreference::ShowInspector(false))
    );
    assert_eq!(app.scope, SearchScope::Global);
    for (event, scope) in [
        (key(KeyCode::Tab), SearchScope::Directory),
        (ctrl('i'), SearchScope::Repository),
    ] {
        assert_eq!(app.handle_key(event), Intent::ReloadScope);
        assert_eq!(app.scope, scope);
        assert!(!app.show_inspector);
    }
    assert_eq!(app.handle_key(key(KeyCode::BackTab)), Intent::ReloadScope);
    assert_eq!(app.scope, SearchScope::Directory);
    let original = app.query.clone();
    app.handle_key(key(KeyCode::Char('?')));
    assert_eq!(app.view, View::Help);
    assert_eq!(app.query, original);
    app.handle_key(key(KeyCode::Esc));
    for number in 1..=6 {
        assert_eq!(app.handle_key(key(KeyCode::F(number))), Intent::None);
        assert_eq!(app.view, View::Main);
    }
    app.handle_paste("?");
    assert_eq!(app.query, format!("{original}?"));
}

#[test]
fn binding_editor_rejects_conflicts_can_cancel_and_resets_defaults() {
    let mut app = app();
    app.handle_key(ctrl('s'));
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Keybindings);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.handle_key(ctrl('t')), Intent::None);
    assert!(app.binding_error.as_ref().unwrap().contains("already used"));
    assert_eq!(app.binding_pending, Some(KeyAction::Inspector));
    assert_eq!(app.handle_key(key(KeyCode::Char('a'))), Intent::None);
    assert!(app.binding_error.as_ref().unwrap().contains("Ctrl"));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.binding_pending, None);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.handle_key(ctrl('e')),
        Intent::SavePreference(UiPreference::Keybinding(
            KeyAction::Inspector,
            KeyBinding::Ctrl('e')
        ))
    );
    assert_eq!(app.binding_pending, None);
    assert_eq!(
        app.handle_key(key(KeyCode::Backspace)),
        Intent::SavePreference(UiPreference::Keybinding(
            KeyAction::Inspector,
            KeyBinding::Ctrl('o')
        ))
    );
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(ctrl('e'));
    assert_eq!(
        app.handle_key(key(KeyCode::Char('r'))),
        Intent::SavePreference(UiPreference::ResetKeybindings)
    );
    assert_eq!(app.preferences.keybindings, KeyBindings::default());
    for _ in 0..5 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(ctrl('f'));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.handle_key(key(KeyCode::Tab)),
        Intent::SavePreference(UiPreference::Keybinding(
            KeyAction::Scope,
            KeyBinding::Ctrl('i')
        ))
    );
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(ctrl('a'));
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Char('?')));
    assert_eq!(app.view, View::Main);
    assert!(app.query.ends_with('?'));
    app.handle_key(ctrl('a'));
    assert_eq!(app.view, View::Help);
}

#[test]
fn bindings_render_conflicts_capture_and_mouse_rows_after_resize() {
    let mut app = app();
    app.open_overlay(View::Settings);
    app.open_overlay(View::Keybindings);
    app.area = Rect::new(0, 0, 40, 12);
    app.handle_key(key(KeyCode::End));
    let rows = bindings::rows(app.area);
    let visible = visible_range(app.binding_selected, KeyAction::ALL.len(), rows.height);
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rows.x,
        row: rows.y + (app.binding_selected - visible.start) as u16,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(event, app.area);
    assert_eq!(app.binding_pending, Some(KeyAction::Help));
    for event in [ctrl('i'), key(KeyCode::Char('?'))] {
        app.handle_key(event);
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert_eq!(app.view, View::Keybindings);
    }
}

#[test]
fn settings_and_scrolled_actions_use_the_same_mouse_geometry_as_rendering() {
    let mut app = app();
    let area = Rect::new(0, 0, 40, 12);
    app.open_overlay(View::Settings);
    app.handle_key(key(KeyCode::End));
    let rows = dashboard::settings_rows(area);
    let visible = visible_range(app.setting_selected, 5, rows.height);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rows.x,
        row: rows.y + (4 - visible.start) as u16,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(app.handle_mouse(click, area), Intent::None);
    assert_eq!(app.view, View::Keybindings);
    app.handle_key(key(KeyCode::Esc));
    app.open_overlay(View::Manage);
    app.action_selected = app.actions().len() - 1;
    let rows = render::menu_rows(area, &app);
    let visible = render::action_range(area, &app);
    let index = app
        .actions()
        .iter()
        .position(|action| *action == Action::Settings)
        .unwrap();
    assert!(visible.contains(&index));
    app.handle_mouse(
        MouseEvent {
            column: rows.x,
            row: rows.y + (index - visible.start) as u16,
            ..click
        },
        area,
    );
    assert_eq!(app.view, View::Settings);
}
