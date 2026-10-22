use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;

use super::render::{dialog, fit, line, popup, row};
use super::theme::Palette;
use super::{App, Intent, visible_range};
use crate::keybindings::KeyAction;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

pub(super) fn rows(area: Rect) -> Rect {
    let area = popup(area, 76, 22);
    Rect::new(
        area.x + 2,
        area.y + 4,
        area.width.saturating_sub(4),
        area.height
            .saturating_sub(8)
            .min(KeyAction::ALL.len() as u16),
    )
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &App, palette: Palette) {
    let area = popup(frame.area(), 76, 22);
    dialog(frame, area, " Keybindings ", palette);
    let inner = area.inner(Margin::new(2, 1));
    line(
        frame,
        row(inner, 1),
        vec![ratatui::text::Span::styled(
            "Select an action, then press Enter to rebind",
            Style::default().fg(palette.muted),
        )],
    );
    let rows = rows(frame.area());
    let visible = visible_range(app.binding_selected, KeyAction::ALL.len(), rows.height);
    for (offset, index) in visible.enumerate() {
        let action = KeyAction::ALL[index];
        let selected = index == app.binding_selected;
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
        let binding = if app.binding_pending == Some(action) {
            "Press shortcut…".into()
        } else {
            app.preferences.keybindings.get(action).to_string()
        };
        let text = if rows.width >= 54 {
            format!(
                "{} {:28} {binding}",
                if selected { "›" } else { " " },
                action.label()
            )
        } else {
            format!(
                "{} {}: {binding}",
                if selected { "›" } else { " " },
                action.label()
            )
        };
        frame.render_widget(
            Paragraph::new(fit(&text, rows.width)).style(style),
            row(rows, offset as u16),
        );
    }
    let (hint, colour) = if let Some(error) = &app.binding_error {
        (error.clone(), palette.error)
    } else if let Some(action) = app.binding_pending {
        (
            format!(
                "Press Ctrl + letter, Tab{} · Esc cancels",
                if action == KeyAction::Help {
                    " or ?"
                } else {
                    ""
                }
            ),
            palette.accent,
        )
    } else {
        (
            "Tab and Ctrl-I share a terminal key code".into(),
            palette.muted,
        )
    };
    line(
        frame,
        row(inner, inner.height.saturating_sub(3)),
        vec![ratatui::text::Span::styled(
            fit(&hint, inner.width),
            Style::default().fg(colour),
        )],
    );
    line(
        frame,
        row(inner, inner.height.saturating_sub(1)),
        vec![ratatui::text::Span::styled(
            fit(
                "↑↓ select · ↵ edit · ⌫ reset · r reset all · esc",
                inner.width,
            ),
            Style::default().fg(palette.muted),
        )],
    );
}

pub(super) fn handle_mouse(app: &mut App, mouse: MouseEvent, area: Rect) -> Intent {
    if app.binding_pending.is_some() {
        return Intent::None;
    }
    let rows = rows(area);
    if rows.contains((mouse.column, mouse.row).into()) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let visible =
                    visible_range(app.binding_selected, KeyAction::ALL.len(), rows.height);
                let index = visible.start + usize::from(mouse.row - rows.y);
                if index < visible.end {
                    app.binding_selected = index;
                    app.binding_pending = Some(KeyAction::ALL[index]);
                    app.binding_error = None;
                }
            }
            MouseEventKind::ScrollUp => {
                app.binding_selected = app.binding_selected.saturating_sub(1)
            }
            MouseEventKind::ScrollDown => {
                app.binding_selected = (app.binding_selected + 1).min(KeyAction::ALL.len() - 1)
            }
            _ => {}
        }
    }
    Intent::None
}
