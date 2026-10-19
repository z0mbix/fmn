use std::path::Path;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::theme::{Palette, THEMES};
use super::{Action, App, Mutation, View, mutation_confirmation, visible_range};

pub(super) struct Geometry {
    header: Rect,
    search: Rect,
    scope: Rect,
    pub results: Rect,
    inspector: Option<Rect>,
    footer: Rect,
}

pub(super) fn geometry(area: Rect, show_inspector: bool) -> Geometry {
    let inner = area.inner(Margin::new(1, 1));
    let chrome_height = if area.height >= 20 { 2 } else { 1 };
    let parts = Layout::vertical([
        Constraint::Length(chrome_height),
        Constraint::Length(3),
        Constraint::Length(chrome_height),
        Constraint::Min(0),
        Constraint::Length(chrome_height),
    ])
    .split(inner);
    let (results, inspector) = if show_inspector && parts[3].width >= 96 && parts[3].height >= 10 {
        let panes = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(parts[3]);
        (panes[0], Some(panes[1]))
    } else if show_inspector && parts[3].height >= 16 {
        let panes = Layout::vertical([Constraint::Min(8), Constraint::Length(8)]).split(parts[3]);
        (panes[0], Some(panes[1]))
    } else {
        (parts[3], None)
    };
    Geometry {
        header: parts[0],
        search: parts[1],
        scope: parts[2],
        results,
        inspector,
        footer: parts[4],
    }
}

pub(super) fn candidate_rows(area: Rect) -> (Rect, u16) {
    let heading_height = if area.height >= 5 { 2 } else { 1 };
    let rows = Rect::new(
        area.x,
        area.y.saturating_add(heading_height).min(area.bottom()),
        area.width,
        area.height.saturating_sub(heading_height),
    );
    let height = if rows.height >= 8 { 2 } else { 1 };
    (rows, height)
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2).max(1));
    let height = height.min(area.height.saturating_sub(2).max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
}

fn details_area(area: Rect) -> Rect {
    popup(
        area,
        area.width.saturating_sub(8).min(100),
        area.height.saturating_sub(4),
    )
}

pub(super) fn details_rows(area: Rect) -> Rect {
    let area = details_area(area);
    Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(4),
        area.width.saturating_sub(4),
        area.height.saturating_sub(7),
    )
}

fn menu_area(area: Rect, app: &App) -> Rect {
    let count = if app.view == View::Themes {
        THEMES.len()
    } else {
        app.actions().len()
    };
    popup(area, 64, count as u16 + 7)
}

pub(super) fn menu_rows(area: Rect, app: &App) -> Rect {
    let area = menu_area(area, app);
    Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(4),
        area.width.saturating_sub(4),
        area.height.saturating_sub(6),
    )
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let palette = Palette::new(app.theme);
    frame.render_widget(Block::default().style(palette.base()), area);
    if area.width < 30 || area.height < 10 {
        draw_small(frame, area, app, palette);
    } else {
        frame.render_widget(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(palette.border)),
            area,
        );
        let layout = geometry(area, app.show_inspector);
        draw_header(frame, layout.header, app, palette);
        draw_search(frame, layout.search, app, palette);
        let scope = if app.current_dir_only {
            "◉ This directory"
        } else {
            "◉ All directories"
        };
        line(
            frame,
            layout.scope,
            vec![Span::styled(
                format!(" {scope}"),
                Style::default().fg(palette.accent),
            )],
        );
        let count = format!(
            "{} {} ",
            app.ranked.len(),
            if app.ranked.len() == 1 {
                "match"
            } else {
                "matches"
            }
        );
        right_line(frame, layout.scope, &count, palette.muted);
        draw_candidates(frame, layout.results, app, palette);
        if let Some(inspector) = layout.inspector {
            draw_inspector(
                frame,
                inspector,
                app,
                palette,
                inspector.y > layout.results.y,
            );
        }
        draw_footer(frame, layout.footer, app, palette);
    }

    if app.view == View::Details || app.return_to_details {
        dim(frame, palette);
        draw_details(frame, app, palette);
    }
    match &app.view {
        View::Main | View::Details => {}
        view => {
            dim(frame, palette);
            match view {
                View::Help => draw_help(frame, app, palette),
                View::Manage => draw_manage(frame, app, palette),
                View::Themes => draw_themes(frame, app, palette),
                View::Confirm { mutation, input } => {
                    draw_confirm(frame, app, mutation, input, palette)
                }
                _ => {}
            }
        }
    }
}

fn draw_small(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    line(
        frame,
        area,
        vec![
            Span::styled("fmn › ", Style::default().fg(palette.accent)),
            Span::raw(display_text(&app.query, false)),
        ],
    );
    if area.height > 1 {
        let command = app
            .selected_candidate()
            .map_or("No matches", |candidate| candidate.command.as_str());
        line(
            frame,
            row(area, 1),
            vec![Span::styled(
                fit(&display_text(command, false), area.width),
                palette.selection(),
            )],
        );
    }
    if area.height > 2 {
        line(
            frame,
            row(area, area.height - 1),
            vec![Span::styled(
                if app.execute_on_select {
                    "↵ run  F2 details  esc cancel"
                } else {
                    "↵ insert  F2 details  esc cancel"
                },
                Style::default().fg(palette.muted),
            )],
        );
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    frame.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(palette.border)),
        area,
    );
    line(
        frame,
        area,
        vec![
            Span::styled(
                " fmn",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" / history", Style::default().fg(palette.muted)),
        ],
    );
    if area.width >= 48 {
        let path = short_path(&app.current_dir, app.home_dir.as_deref());
        right_line(
            frame,
            area,
            &format!("{} ", fit(&path, area.width.saturating_sub(22))),
            palette.muted,
        );
    }
}

fn draw_search(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let border = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette.accent));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    line(
        frame,
        inner,
        vec![Span::styled(" › ", Style::default().fg(palette.accent))],
    );
    let input_area = Rect::new(
        inner.x.saturating_add(3),
        inner.y,
        inner.width.saturating_sub(4),
        1.min(inner.height),
    );
    if app.query.is_empty() {
        line(
            frame,
            input_area,
            vec![Span::styled(
                "Search your command history",
                Style::default().fg(palette.muted),
            )],
        );
    } else {
        let (text, _) = search_window(app, input_area.width);
        line(frame, input_area, vec![Span::raw(text)]);
    }
    if app.view == View::Main && input_area.width > 0 && input_area.height > 0 {
        let (_, offset) = search_window(app, input_area.width);
        frame.set_cursor_position(Position::new(input_area.x + offset, input_area.y));
    }
}

fn search_window(app: &App, width: u16) -> (String, u16) {
    let max = usize::from(width.saturating_sub(1));
    let mut start = 0;
    let mut before = display_text(&app.query[..app.cursor], false).width();
    for (index, grapheme) in app.query[..app.cursor].grapheme_indices(true) {
        if before <= max {
            break;
        }
        before = before.saturating_sub(display_text(grapheme, false).width());
        start = index + grapheme.len();
    }
    (
        fit(&display_text(&app.query[start..], false), width),
        before.min(max) as u16,
    )
}

fn draw_candidates(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    line(
        frame,
        area,
        vec![Span::styled(
            " COMMANDS",
            Style::default().fg(palette.muted),
        )],
    );
    if area.width >= 40 {
        right_line(frame, area, "LAST USED ", palette.muted);
    }
    let (rows, height) = candidate_rows(area);
    if app.ranked.is_empty() {
        let message = if app.candidates.is_empty() {
            "Your history starts here. Run a command, then Ctrl-R."
        } else {
            "No matches. Try another query or change directory scope."
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            rows.inner(Margin::new(1, 1)),
        );
        return;
    }
    let visible = visible_range(app.selected, app.ranked.len(), rows.height / height);
    let mut matcher = (!app.query.is_empty()).then(|| Matcher::new(Config::DEFAULT.match_paths()));
    let pattern = Pattern::parse(&app.query, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut indices = Vec::new();
    for (offset, index) in visible.enumerate() {
        let candidate = &app.candidates[app.ranked[index].index];
        let selected = index == app.selected;
        let row_area = Rect::new(rows.x, rows.y + offset as u16 * height, rows.width, height);
        let style = if selected {
            palette.selection()
        } else {
            palette.base()
        };
        frame.render_widget(Block::default().style(style), row_area);
        line(
            frame,
            row_area,
            vec![Span::styled(
                if selected { " ›" } else { "  " },
                Style::default().fg(palette.accent),
            )],
        );
        let age = relative_time(candidate.last_seen_at, app.now_millis);
        let age_width = if row_area.width >= 40 {
            age.width() as u16 + 2
        } else {
            0
        };
        let command_area = Rect::new(
            row_area.x + 3,
            row_area.y,
            row_area.width.saturating_sub(4 + age_width),
            1,
        );
        indices.clear();
        if let Some(matcher) = &mut matcher {
            pattern.indices(
                Utf32Str::new(&candidate.command, &mut buffer),
                matcher,
                &mut indices,
            );
            indices.sort_unstable();
            indices.dedup();
        }
        line(
            frame,
            command_area,
            highlighted(&candidate.command, &indices, command_area.width, palette),
        );
        if age_width > 0 {
            right_line(frame, row_area, &format!("{age} "), palette.muted);
        }
        if height == 2 {
            let (symbol, color) = exit_style(candidate.latest_exit_code, palette);
            let path = short_path(&candidate.latest_cwd, app.home_dir.as_deref());
            line(
                frame,
                Rect::new(
                    row_area.x + 3,
                    row_area.y + 1,
                    row_area.width.saturating_sub(4),
                    1,
                ),
                vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(color)),
                    Span::styled(
                        format!(
                            "{} · ×{}",
                            fit(&path, row_area.width.saturating_sub(20)),
                            candidate.use_count
                        ),
                        Style::default().fg(palette.muted),
                    ),
                ],
            );
        }
    }
}

fn highlighted(command: &str, indices: &[u32], width: u16, palette: Palette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut used = 0;
    let total = display_text(command, false).width();
    let max = usize::from(width).saturating_sub(usize::from(total > usize::from(width)));
    for (index, grapheme) in command.graphemes(true).enumerate() {
        let text = display_text(grapheme, false);
        if used + text.width() > max {
            break;
        }
        used += text.width();
        let style = if indices.binary_search(&(index as u32)).is_ok() {
            Style::default()
                .fg(palette.matched)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
        };
        spans.push(Span::styled(text, style));
    }
    if total > usize::from(width) && width > 0 {
        spans.push(Span::styled("…", Style::default().fg(palette.muted)));
    }
    spans
}

fn draw_inspector(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette, stacked: bool) {
    let block = Block::default()
        .style(Style::default().bg(palette.panel))
        .borders(if stacked { Borders::TOP } else { Borders::LEFT })
        .border_style(Style::default().fg(palette.border));
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    line(
        frame,
        inner,
        vec![Span::styled(
            "INSPECTOR",
            Style::default().fg(palette.muted),
        )],
    );
    if inner.width >= 20 {
        right_line(frame, inner, "F2 expand", palette.accent);
    }
    let Some(candidate) = app.selected_candidate() else {
        line(
            frame,
            row(inner, 2),
            vec![Span::styled(
                "Select a command to inspect.",
                Style::default().fg(palette.muted),
            )],
        );
        return;
    };
    let recent = if app.preview_command_id == Some(candidate.id) {
        app.preview.as_slice()
    } else {
        &[]
    };
    let preview = recent
        .first()
        .filter(|entry| entry.command_id == candidate.id);
    let command = display_text(&candidate.command, true);
    if inner.height < 12 {
        line(
            frame,
            row(inner, 2),
            vec![Span::styled(
                fit(&display_text(&candidate.command, false), inner.width),
                Style::default().fg(palette.accent),
            )],
        );
        let path = short_path(&candidate.latest_cwd, app.home_dir.as_deref());
        line(
            frame,
            row(inner, 3),
            vec![Span::styled(
                fit(&path, inner.width),
                Style::default().fg(palette.muted),
            )],
        );
        line(
            frame,
            row(inner, 4),
            vec![Span::raw(format!(
                "{} runs · {} · {}",
                candidate.use_count,
                relative_time(candidate.last_seen_at, app.now_millis),
                format_duration(preview.and_then(|entry| entry.duration_ms))
            ))],
        );
        return;
    }
    let command_lines: usize = command
        .lines()
        .map(|text| {
            text.width()
                .div_ceil(usize::from(inner.width.saturating_sub(2).max(1)))
                .max(1)
        })
        .sum();
    let command_height = command_lines.clamp(1, 4) as u16 + 2;
    frame.render_widget(
        Paragraph::new(command)
            .style(Style::default().fg(palette.accent))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(palette.border)),
            )
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y + 2, inner.width, command_height),
    );
    let facts_start = command_height + 3;
    let directory = short_path(&candidate.latest_cwd, app.home_dir.as_deref());
    let exit = preview.map_or(candidate.latest_exit_code, |entry| entry.exit_code);
    let (symbol, color) = exit_style(exit, palette);
    for (offset, (label, value, color)) in [
        ("Directory", directory, palette.text),
        (
            "Last run",
            relative_time(candidate.last_seen_at, app.now_millis),
            palette.text,
        ),
        (
            "Duration",
            format_duration(preview.and_then(|entry| entry.duration_ms)),
            palette.text,
        ),
        (
            "Exit code",
            format!(
                "{symbol} {}",
                exit.map_or_else(|| "running / unknown".into(), |code| code.to_string())
            ),
            color,
        ),
        (
            "Used",
            format!(
                "{} {}",
                candidate.use_count,
                if candidate.use_count == 1 {
                    "time"
                } else {
                    "times"
                }
            ),
            palette.text,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        line(
            frame,
            row(inner, facts_start + offset as u16),
            vec![
                Span::styled(format!("{label:10}"), Style::default().fg(palette.muted)),
                Span::styled(
                    fit(&value, inner.width.saturating_sub(10)),
                    Style::default().fg(color),
                ),
            ],
        );
    }
    let recent_start = facts_start + 7;
    if inner.height > recent_start + 1 {
        line(
            frame,
            row(inner, recent_start),
            vec![Span::styled(
                "RECENT OCCURRENCES",
                Style::default().fg(palette.muted),
            )],
        );
        for (offset, entry) in recent.iter().enumerate() {
            let y = recent_start + 2 + offset as u16 * 3;
            if y + 1 >= inner.height {
                break;
            }
            let (symbol, color) = exit_style(entry.exit_code, palette);
            line(
                frame,
                row(inner, y),
                vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(color)),
                    Span::raw(relative_time(entry.started_at, app.now_millis)),
                ],
            );
            right_line(
                frame,
                row(inner, y),
                &format_duration(entry.duration_ms),
                palette.muted,
            );
            line(
                frame,
                row(inner, y + 1),
                vec![Span::styled(
                    format!(
                        "  {} · {}",
                        short_path(&entry.cwd, app.home_dir.as_deref()),
                        display_text(&entry.shell, false)
                    ),
                    Style::default().fg(palette.muted),
                )],
            );
        }
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(palette.border)),
        area,
    );
    let accept = if app.execute_on_select {
        " run  "
    } else {
        " insert  "
    };
    let items = if area.width >= 92 {
        vec![
            ("↑↓", " navigate  "),
            ("↵", accept),
            ("Tab", " scope  "),
            ("F2", " details  "),
            ("^X", " actions  "),
            ("F3", " theme  "),
            ("F1", " help"),
        ]
    } else if area.width >= 62 {
        vec![
            ("↑↓", " move  "),
            ("↵", accept),
            ("Tab", " scope  "),
            ("F2", " details  "),
            ("^X", " actions  "),
            ("F1", " help"),
        ]
    } else {
        vec![
            ("↵", accept),
            ("^X", " actions  "),
            ("F1", " help  "),
            ("esc", " cancel"),
        ]
    };
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in items {
        spans.extend([
            Span::styled(key, Style::default().fg(palette.accent)),
            Span::styled(label, Style::default().fg(palette.muted)),
        ]);
    }
    let offset = u16::from(area.height > 1);
    line(frame, row(area, offset), spans);
    if area.width >= 112 {
        let name = THEMES
            .iter()
            .find(|(theme, _)| *theme == app.theme)
            .map_or("Mint", |(_, name)| *name);
        right_line(
            frame,
            row(area, offset),
            &format!("{name} · esc cancel "),
            palette.muted,
        );
    }
}

fn draw_details(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = details_area(frame.area());
    dialog(frame, area, " Occurrences ", palette);
    let inner = area.inner(Margin::new(2, 1));
    if let Some(candidate) = app.selected_candidate() {
        line(
            frame,
            row(inner, 1),
            vec![Span::styled(
                fit(&display_text(&candidate.command, false), inner.width),
                Style::default().fg(palette.accent),
            )],
        );
    }
    let rows = details_rows(frame.area());
    if app.details.is_empty() {
        line(
            frame,
            rows,
            vec![Span::styled(
                "No occurrences remain.",
                Style::default().fg(palette.muted),
            )],
        );
    }
    let visible = visible_range(app.detail_selected, app.details.len(), rows.height / 2);
    for (offset, index) in visible.enumerate() {
        let entry = &app.details[index];
        let area = Rect::new(rows.x, rows.y + offset as u16 * 2, rows.width, 2);
        let selected = index == app.detail_selected;
        if selected {
            frame.render_widget(Block::default().style(palette.selection()), area);
        }
        let (symbol, color) = exit_style(entry.exit_code, palette);
        line(
            frame,
            area,
            vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(palette.accent),
                ),
                Span::styled(format!("{symbol} "), Style::default().fg(color)),
                Span::raw(format!(
                    "{} · {} · exit {}",
                    relative_time(entry.started_at, app.now_millis),
                    format_duration(entry.duration_ms),
                    entry
                        .exit_code
                        .map_or_else(|| "?".into(), |code| code.to_string())
                )),
            ],
        );
        line(
            frame,
            row(area, 1),
            vec![Span::styled(
                format!(
                    "    {} · {}",
                    short_path(&entry.cwd, app.home_dir.as_deref()),
                    display_text(&entry.shell, false)
                ),
                Style::default().fg(palette.muted),
            )],
        );
    }
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![Span::styled(
            if app.execute_on_select {
                "↑↓ select · ↵ run · ^X actions · esc back"
            } else {
                "↑↓ select · ↵ insert · ^X actions · esc back"
            },
            Style::default().fg(palette.muted),
        )],
    );
}

fn draw_help(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = popup(frame.area(), 64, 18);
    dialog(frame, area, " Help ", palette);
    let inner = area.inner(Margin::new(2, 1));
    let text = [
        ("Type", "Fuzzy-search command history"),
        ("← → / Home End", "Move the search cursor"),
        ("Ctrl-U", "Clear the query"),
        ("↑ ↓ / Ctrl-P N", "Select a command"),
        (
            "Enter",
            if app.execute_on_select {
                "Run in the current shell"
            } else {
                "Insert without executing"
            },
        ),
        ("Tab", "Toggle directory scope"),
        ("F2", "Inspect all occurrences"),
        ("Ctrl-X", "Open command actions"),
        ("F3", "Choose a colour theme"),
        ("F4", "Toggle the inspector"),
        ("Mouse", "Select results / scroll lists"),
        ("Esc / Ctrl-C", "Close / cancel"),
    ];
    for (offset, (key, label)) in text.into_iter().enumerate() {
        line(
            frame,
            row(inner, offset as u16 + 1),
            vec![
                Span::styled(format!("{key:17}"), Style::default().fg(palette.accent)),
                Span::raw(label),
            ],
        );
    }
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![Span::styled(
            "Press any key to return",
            Style::default().fg(palette.muted),
        )],
    );
}

fn draw_manage(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = menu_area(frame.area(), app);
    dialog(frame, area, " Command actions ", palette);
    let command = app
        .selected_candidate()
        .map_or("No command selected", |candidate| {
            candidate.command.as_str()
        });
    line(
        frame,
        Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1),
        vec![Span::styled(
            fit(&display_text(command, false), area.width.saturating_sub(4)),
            Style::default().fg(palette.muted),
        )],
    );
    let rows = menu_rows(frame.area(), app);
    for (index, action) in app.actions().into_iter().enumerate() {
        if index as u16 >= rows.height {
            break;
        }
        let (label, key, destructive) = match action {
            Action::Insert => (
                if app.execute_on_select {
                    "Run in current shell"
                } else {
                    "Insert into prompt"
                },
                "↵",
                false,
            ),
            Action::Inspect => ("Inspect occurrences", "i / F2", false),
            Action::Theme => ("Change colour theme", "t", false),
            Action::Delete => ("Delete selected occurrence", "d", true),
            Action::Forget => ("Forget command everywhere", "f", true),
            Action::Clear => ("Clear all history", "c", true),
        };
        let area = row(rows, index as u16);
        if index == app.action_selected {
            frame.render_widget(Block::default().style(palette.selection()), area);
        }
        line(
            frame,
            area,
            vec![
                Span::styled(
                    if index == app.action_selected {
                        "› "
                    } else {
                        "  "
                    },
                    Style::default().fg(palette.accent),
                ),
                Span::styled(
                    fit(label, area.width.saturating_sub(10)),
                    Style::default().fg(if destructive {
                        palette.error
                    } else {
                        palette.text
                    }),
                ),
            ],
        );
        right_line(frame, area, key, palette.accent);
    }
    line(
        frame,
        Rect::new(
            area.x + 2,
            area.bottom().saturating_sub(2),
            area.width.saturating_sub(4),
            1,
        ),
        vec![Span::styled(
            "↑↓ select · ↵ choose · esc close",
            Style::default().fg(palette.muted),
        )],
    );
}

fn draw_themes(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = menu_area(frame.area(), app);
    dialog(frame, area, " Colour theme ", palette);
    line(
        frame,
        Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1),
        vec![Span::styled(
            "Preview a palette for this search",
            Style::default().fg(palette.muted),
        )],
    );
    let rows = menu_rows(frame.area(), app);
    for (index, (theme, name)) in THEMES.iter().enumerate() {
        if index as u16 >= rows.height {
            break;
        }
        let area = row(rows, index as u16);
        if *theme == app.theme {
            frame.render_widget(Block::default().style(palette.selection()), area);
        }
        line(
            frame,
            area,
            vec![Span::styled(
                format!("{} {name}", if *theme == app.theme { "›" } else { " " }),
                Style::default().fg(palette.text),
            )],
        );
        right_line(frame, area, "● ● ●", Palette::new(*theme).accent);
    }
    line(
        frame,
        Rect::new(
            area.x + 2,
            area.bottom().saturating_sub(2),
            area.width.saturating_sub(4),
            1,
        ),
        vec![Span::styled(
            "↑↓ preview · ↵ choose · esc cancel",
            Style::default().fg(palette.muted),
        )],
    );
}

fn draw_confirm(
    frame: &mut Frame<'_>,
    app: &App,
    mutation: &Mutation,
    input: &str,
    palette: Palette,
) {
    let area = popup(frame.area(), 64, 14);
    dialog(frame, area, " Confirm permanent removal ", palette);
    let inner = area.inner(Margin::new(2, 1));
    let description = match mutation {
        Mutation::DeleteEntry(_) => "Delete this occurrence from history?",
        Mutation::ForgetCommand(_) => "Forget this command and all its occurrences?",
        Mutation::Clear => "Permanently remove all command history?",
    };
    line(
        frame,
        row(inner, 1),
        vec![Span::styled(
            description,
            Style::default().fg(palette.error),
        )],
    );
    if *mutation != Mutation::Clear
        && let Some(candidate) = app.selected_candidate()
    {
        line(
            frame,
            row(inner, 3),
            vec![Span::raw(fit(
                &display_text(&candidate.command, false),
                inner.width,
            ))],
        );
    }
    let expected = mutation_confirmation(mutation);
    line(
        frame,
        row(inner, 5),
        vec![Span::raw(format!("Type {expected} and press Enter:"))],
    );
    let input_area = Rect::new(
        inner.x,
        inner.y + 6,
        inner.width,
        3.min(inner.height.saturating_sub(6)),
    );
    let color = if input == expected {
        palette.success
    } else {
        palette.accent
    };
    frame.render_widget(
        Paragraph::new(format!(" {}", display_text(input, false))).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color)),
        ),
        input_area,
    );
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![Span::styled(
            "esc cancel · history cannot be restored",
            Style::default().fg(palette.muted),
        )],
    );
    if input_area.height >= 3 && input_area.width >= 4 {
        frame.set_cursor_position(Position::new(
            input_area.x
                + 2
                + display_text(input, false)
                    .width()
                    .min(usize::from(input_area.width.saturating_sub(4))) as u16,
            input_area.y + 1,
        ));
    }
}

fn dialog(frame: &mut Frame<'_>, area: Rect, title: &str, palette: Palette) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .title(title.to_owned())
            .border_style(Style::default().fg(palette.accent))
            .style(
                palette
                    .base()
                    .bg(palette.panel)
                    .remove_modifier(Modifier::DIM),
            ),
        area,
    );
}

fn dim(frame: &mut Frame<'_>, palette: Palette) {
    let area = frame.area();
    frame.buffer_mut().set_style(
        area,
        Style::default()
            .fg(palette.muted)
            .add_modifier(Modifier::DIM),
    );
}

fn row(area: Rect, offset: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(offset).min(area.bottom()),
        area.width,
        u16::from(offset < area.height),
    )
}

fn line(frame: &mut Frame<'_>, area: Rect, spans: Vec<Span<'_>>) {
    if area.height > 0 {
        frame.render_widget(Line::from(spans), Rect::new(area.x, area.y, area.width, 1));
    }
}

fn right_line(frame: &mut Frame<'_>, area: Rect, text: &str, color: ratatui::style::Color) {
    if area.height > 0 {
        let text = fit(text, area.width);
        let width = text.width() as u16;
        frame.render_widget(
            Line::styled(text, Style::default().fg(color)),
            Rect::new(area.right().saturating_sub(width), area.y, width, 1),
        );
    }
}

fn exit_style(code: Option<i32>, palette: Palette) -> (&'static str, ratatui::style::Color) {
    match code {
        Some(0) => ("✓", palette.success),
        Some(_) => ("✗", palette.error),
        None => ("·", palette.muted),
    }
}

fn display_text(text: &str, multiline: bool) -> String {
    text.chars()
        .map(|character| match character {
            '\n' if multiline => "\n".into(),
            '\n' => " ↵ ".into(),
            '\t' => "    ".into(),
            '\x1b' => "␛".into(),
            c if c.is_control() => "�".into(),
            c => c.to_string(),
        })
        .collect()
}

fn short_path(path: &str, home: Option<&str>) -> String {
    let shortened = home
        .filter(|home| !home.is_empty())
        .and_then(|home| Path::new(path).strip_prefix(home).ok())
        .map(|tail| {
            if tail.as_os_str().is_empty() {
                "~".into()
            } else {
                format!("~/{}", tail.display())
            }
        })
        .unwrap_or_else(|| path.into());
    display_text(&shortened, false)
}

fn fit(text: &str, width: u16) -> String {
    let width = usize::from(width);
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        if used + grapheme.width() >= width {
            break;
        }
        used += grapheme.width();
        result.push_str(grapheme);
    }
    result.push('…');
    result
}

fn relative_time(timestamp: i64, now: i64) -> String {
    let seconds = now.saturating_sub(timestamp).max(0) / 1_000;
    match seconds {
        0..60 => "just now".into(),
        60..3_600 => format!("{}m ago", seconds / 60),
        3_600..86_400 => format!("{}h ago", seconds / 3_600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

fn format_duration(duration: Option<i64>) -> String {
    match duration {
        None => "—".into(),
        Some(ms) if ms < 0 => "—".into(),
        Some(ms) if ms < 1_000 => format!("{ms}ms"),
        Some(ms) if ms < 60_000 => format!("{:.1}s", ms as f64 / 1_000.0),
        Some(ms) if ms < 3_600_000 => format!("{}m {}s", ms / 60_000, ms / 1_000 % 60),
        Some(ms) => format!("{}h {}m", ms / 3_600_000, ms / 60_000 % 60),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::model::{Entry, SearchCandidate};

    fn app() -> App {
        let mut app = App::new(
            vec![SearchCandidate {
                id: 1,
                command: "cargo test --all-features".into(),
                first_seen_at: 1,
                last_seen_at: 1_000,
                use_count: 36,
                cwd_use_count: 2,
                cwd_last_seen_at: Some(1_000),
                latest_cwd: "/home/demo/work/fmn".into(),
                latest_exit_code: Some(0),
            }],
            String::new(),
            121_000,
        );
        app.home_dir = Some("/home/demo".into());
        app.current_dir = "/home/demo/work/fmn".into();
        app.set_preview(
            1,
            vec![Entry {
                id: 1,
                command_id: 1,
                command: "cargo test --all-features".into(),
                started_at: 1_000,
                finished_at: Some(2_800),
                duration_ms: Some(1_800),
                exit_code: Some(0),
                cwd: "/home/demo/work/fmn".into(),
                shell: "zsh".into(),
                session_id: "demo".into(),
                source: "live".into(),
            }],
        );
        app
    }

    fn text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn focus_shows_real_metadata_and_responsive_inspector() {
        let app = app();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let rendered = text(&terminal);
        for expected in [
            "cargo test --all-features",
            "INSPECTOR",
            "1.8s",
            "36 times",
            "RECENT OCCURRENCES",
            "~/work/fmn",
            "zsh",
        ] {
            assert!(rendered.contains(expected), "{expected} missing");
        }
        let wide = geometry(Rect::new(0, 0, 120, 40), true);
        assert!(wide.inspector.unwrap().x > wide.results.x);
        let narrow = geometry(Rect::new(0, 0, 80, 40), true);
        assert!(narrow.inspector.unwrap().y > narrow.results.y);
        assert!(geometry(Rect::new(0, 0, 80, 24), true).inspector.is_none());
        assert!(
            geometry(Rect::new(0, 0, 120, 40), false)
                .inspector
                .is_none()
        );
    }

    #[test]
    fn action_dialog_dims_background_but_preserves_results() {
        let mut app = app();
        app.open_overlay(View::Manage);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let rendered = text(&terminal);
        for expected in [
            "cargo test --all-features",
            "Command actions",
            "Forget command everywhere",
            "esc close",
        ] {
            assert!(rendered.contains(expected));
        }
        let layout = geometry(Rect::new(0, 0, 120, 40), true);
        let (rows, _) = candidate_rows(layout.results);
        let buffer = terminal.backend().buffer();
        assert!(
            buffer[(rows.x + 3, rows.y)]
                .modifier
                .contains(Modifier::DIM)
        );
        let menu = menu_rows(Rect::new(0, 0, 120, 40), &app);
        assert!(
            !buffer[(menu.x + 3, menu.y)]
                .modifier
                .contains(Modifier::DIM)
        );
    }

    #[test]
    fn execution_mode_is_visible_in_the_footer_actions_and_help() {
        let mut app = app();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(text(&terminal).contains("↵ insert"));
        app.execute_on_select = true;
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(text(&terminal).contains("↵ run"));
        app.open_overlay(View::Manage);
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(text(&terminal).contains("Run in current shell"));
        app.view = View::Help;
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(text(&terminal).contains("Run in the current shell"));
    }

    #[test]
    fn highlights_follow_matcher_indices_and_unicode_graphemes() {
        let mut app = app();
        app.query = "'cargo".into();
        app.cursor = app.query.len();
        app.refresh();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let matched: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
            .map(|cell| cell.symbol())
            .collect();
        assert_eq!(matched, "cargo");
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
                .all(|cell| cell.fg == Palette::new(app.theme).matched)
        );
        let palette = Palette::new(app.theme);
        let spans = highlighted("a👩‍💻e\u{301}中", &[1, 2], 12, palette);
        let matched: String = spans
            .iter()
            .filter(|span| span.style.add_modifier.contains(Modifier::UNDERLINED))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(matched, "👩‍💻e\u{301}");
        assert!(fit("a👩‍💻e\u{301}中", 4).width() <= 4);
        assert!(!display_text("\x1b[31mhello\r\nworld", true).contains('\x1b'));
        assert_eq!(
            short_path("/home/demolition/repo", Some("/home/demo")),
            "/home/demolition/repo"
        );
    }

    #[test]
    fn long_query_keeps_the_cursor_inside_the_visible_input() {
        let mut app = app();
        app.query = "界👩‍💻".repeat(100);
        app.cursor = app.query.len();
        for width in 1..30 {
            let (visible, cursor) = search_window(&app, width);
            assert!(visible.width() <= usize::from(width));
            assert!(cursor < width);
        }
    }

    #[test]
    fn display_times_are_readable_and_handle_missing_or_future_values() {
        assert_eq!(relative_time(60_000, 0), "just now");
        assert_eq!(relative_time(0, 120_000), "2m ago");
        assert_eq!(relative_time(0, 3_600_000), "1h ago");
        assert_eq!(relative_time(0, 86_400_000), "1d ago");
        for (value, expected) in [
            (None, "—"),
            (Some(-1), "—"),
            (Some(42), "42ms"),
            (Some(1_800), "1.8s"),
            (Some(62_000), "1m 2s"),
            (Some(3_660_000), "1h 1m"),
        ] {
            assert_eq!(format_duration(value), expected);
        }
    }
}
