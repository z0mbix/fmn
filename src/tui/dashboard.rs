use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::render::{dialog, display_text, fit, format_duration, line, popup, row};
use super::theme::{Palette, THEMES};
use super::{App, View};
use crate::keybindings::KeyAction;
use crate::model::CommandStats;

fn stats_content(area: Rect) -> (Rect, Rect) {
    let area = popup(area, 104, 37);
    let inner = area.inner(Margin::new(2, 1));
    let content = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(4),
    );
    (area, content)
}

fn stats_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let palette = Palette::new(app.theme);
    let Some(stats) = &app.stats else {
        return vec![Line::from("Loading statistics…")];
    };
    let muted = Style::default().fg(palette.muted);
    let accent = Style::default()
        .fg(palette.accent)
        .add_modifier(Modifier::BOLD);
    let rate = if stats.completed == 0 {
        "—".into()
    } else {
        format!(
            "{:.1}%",
            100.0 * stats.failed as f64 / stats.completed as f64
        )
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "{} {}",
                    stats.total,
                    if stats.total == 1 {
                        "execution"
                    } else {
                        "executions"
                    }
                ),
                accent,
            ),
            Span::raw(format!("  ·  {} unique commands", stats.unique)),
        ]),
        Line::from(vec![
            Span::styled(format!("{rate} failed"), Style::default().fg(palette.error)),
            Span::styled(
                format!(
                    "  ·  {} known / {} unknown outcomes",
                    stats.completed,
                    stats.total - stats.completed
                ),
                muted,
            ),
        ]),
        Line::from(vec![
            Span::styled("Average duration  ", muted),
            Span::raw(format_duration(
                stats.average_duration_ms.map(|duration| duration as i64),
            )),
        ]),
        Line::default(),
        Line::from(Span::styled("ACTIVITY · LAST 30 UTC DAYS", accent)),
    ];
    let peak = stats.activity.iter().copied().max().unwrap_or(0);
    let max = peak.max(1);
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    lines.push(Line::from(
        stats
            .activity
            .iter()
            .map(|&count| {
                let symbol = if count == 0 {
                    '·'
                } else {
                    bars[((count as f64 / max as f64 * 7.0).round() as usize).min(7)]
                };
                Span::styled(symbol.to_string(), if count == 0 { muted } else { accent })
            })
            .collect::<Vec<_>>(),
    ));
    lines.push(Line::from(Span::styled(
        format!("29 days ago → today  ·  peak {peak}/day"),
        muted,
    )));
    lines.push(Line::default());
    for (title, commands, kind) in [
        ("MOST USED", &stats.top_commands, 0),
        ("FAILED COMMANDS", &stats.failed_commands, 1),
        ("SLOWEST · AVERAGE DURATION", &stats.slow_commands, 2),
    ] {
        lines.push(Line::from(Span::styled(title, accent)));
        if commands.is_empty() {
            lines.push(Line::from(Span::styled("No recorded commands", muted)));
        }
        for command in commands {
            lines.push(command_line(command, kind, width, palette));
        }
        lines.push(Line::default());
    }
    lines
}

fn command_line(
    command: &CommandStats,
    kind: usize,
    width: u16,
    palette: Palette,
) -> Line<'static> {
    let metric = match kind {
        0 => format!("{}×", command.count),
        1 => format!("{} failed", command.failures),
        _ => format_duration(command.average_duration_ms.map(|duration| duration as i64)),
    };
    let available = width.saturating_sub(14);
    Line::from(vec![
        Span::styled(
            format!("{metric:>12}  "),
            Style::default().fg(if kind == 1 {
                palette.error
            } else {
                palette.accent
            }),
        ),
        Span::raw(fit(&display_text(&command.command, false), available)),
    ])
}

pub(super) fn stats_line_count(app: &App) -> u16 {
    app.stats.as_ref().map_or(1, |stats| {
        8 + [
            &stats.top_commands,
            &stats.failed_commands,
            &stats.slow_commands,
        ]
        .iter()
        .map(|commands| 2 + commands.len().max(1) as u16)
        .sum::<u16>()
    })
}

pub(super) fn stats_max_scroll(app: &App) -> u16 {
    stats_line_count(app).saturating_sub(stats_content(app.area).1.height)
}

pub(super) fn draw_stats(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let (area, content) = stats_content(frame.area());
    dialog(frame, area, " History statistics ", palette);
    let inner = area.inner(Margin::new(2, 1));
    line(
        frame,
        row(inner, 0),
        vec![Span::styled(
            fit(app.scope.label(), inner.width),
            Style::default().fg(palette.accent),
        )],
    );
    frame.render_widget(
        Paragraph::new(stats_lines(app, content.width)).scroll((
            app.stats_scroll
                .min(stats_line_count(app).saturating_sub(content.height)),
            0,
        )),
        content,
    );
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![Span::styled(
            format!(
                "↑↓ scroll · {} scope · r refresh · esc",
                app.preferences
                    .keybindings
                    .get(KeyAction::Scope)
                    .short_label()
            ),
            Style::default().fg(palette.muted),
        )],
    );
}

pub(super) fn settings_rows(area: Rect) -> Rect {
    let area = popup(area, 76, 19);
    Rect::new(
        area.x + 2,
        area.y + 4,
        area.width.saturating_sub(4),
        area.height.saturating_sub(8).min(5),
    )
}

pub(super) fn draw_settings(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = popup(frame.area(), 76, 19);
    dialog(frame, area, " Settings ", palette);
    let inner = area.inner(Margin::new(2, 1));
    line(
        frame,
        row(inner, 1),
        vec![Span::styled(
            "Changes save automatically",
            Style::default().fg(palette.muted),
        )],
    );
    let theme = THEMES
        .iter()
        .find(|(theme, _)| *theme == app.theme)
        .map_or("Mint", |(_, name)| *name);
    let values = [
        ("Colour theme", theme),
        (
            "Show inspector",
            if app.show_inspector { "On" } else { "Off" },
        ),
        ("Default scope", app.preferences.default_scope.label()),
        (
            "Run selection immediately",
            if app.preferences.execute_on_select {
                "On"
            } else {
                "Off (insert into prompt)"
            },
        ),
        ("Keybindings", "Edit shortcuts…"),
    ];
    let rows = settings_rows(frame.area());
    let visible = super::visible_range(app.setting_selected, values.len(), rows.height);
    for (offset, index) in visible.enumerate() {
        let (label, value) = values[index];
        let selected = index == app.setting_selected;
        let palette = if selected {
            palette.selected_content()
        } else {
            palette
        };
        let style = if selected {
            palette.selection()
        } else {
            Style::default().fg(palette.text).bg(palette.panel)
        };
        let area = row(rows, offset as u16);
        let text = if area.width < 54 {
            format!("{} {label}: {value}", if selected { "›" } else { " " })
        } else {
            format!("{} {label:28} {value}", if selected { "›" } else { " " })
        };
        frame.render_widget(Paragraph::new(fit(&text, area.width)).style(style), area);
    }
    let hint = match app.setting_selected {
        0 => "Open the searchable theme picker",
        1 => "Show command metadata beside the results",
        2 if !app.context.available(app.preferences.default_scope) => {
            "Unavailable here; uses global scope until available"
        }
        2 => "Applies now and when you next open fmn",
        3 => "Enter runs commands when using shell integration",
        _ => "Reassign shortcuts; each change saves automatically",
    };
    line(
        frame,
        row(inner, inner.height.saturating_sub(3)),
        vec![Span::styled(
            fit(hint, inner.width),
            Style::default().fg(palette.muted),
        )],
    );
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![Span::styled(
            "↑↓ select · ↵ / ←→ change · esc close",
            Style::default().fg(palette.muted),
        )],
    );
}

pub(super) fn handle_mouse(
    app: &mut App,
    mouse: crossterm::event::MouseEvent,
    area: Rect,
) -> super::Intent {
    use crossterm::event::{MouseButton, MouseEventKind};
    if app.view == View::Stats {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.stats_scroll = app.stats_scroll.saturating_sub(3),
            MouseEventKind::ScrollDown => {
                app.stats_scroll = app
                    .stats_scroll
                    .saturating_add(3)
                    .min(stats_max_scroll(app))
            }
            _ => {}
        }
    } else if app.view == View::Settings {
        let rows = settings_rows(area);
        if rows.contains((mouse.column, mouse.row).into()) {
            let visible = super::visible_range(app.setting_selected, 5, rows.height);
            let index = visible.start + usize::from(mouse.row - rows.y);
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) if index < visible.end => {
                    app.setting_selected = index;
                    return app.activate_setting(false);
                }
                MouseEventKind::ScrollUp => {
                    app.setting_selected = app.setting_selected.saturating_sub(1)
                }
                MouseEventKind::ScrollDown => {
                    app.setting_selected = (app.setting_selected + 1).min(4)
                }
                _ => {}
            }
        }
    }
    super::Intent::None
}
