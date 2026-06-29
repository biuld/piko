//! Top-level render: flat layout engine.
//!
//! Follows the architecture.md design:
//! 1. Compute `LayoutMode` from AppState
//! 2. Build constraints via pure `build_constraints()`
//! 3. Split area via `Layout::vertical()`
//! 4. Delegate each slot to its surface widget
//!
//! All visible elements participate in the layout — no floaters.

use ratatui::{
    Frame,
    layout::{Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::{
    app::{AppMode, AppState},
    input::completion::Completion,
    layout::{
        LayoutMode, agent_panel_height, build_constraints, has_visible_notification,
        has_visible_suggestions,
    },
    notification::NotificationLevel,
    panels::{
        bottom_bar::BottomBar, help::HelpPanel, status::StatusPanel, agent::AgentPanel,
    },
};

/// Main render entry point.
pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    let mode = LayoutMode::from_app(app);
    let agent_h = agent_panel_height(app);
    let has_notif = has_visible_notification(app);
    let has_sugg = has_visible_suggestions(app);
    let sugg_count = if has_sugg {
        app.completions.len()
    } else {
        0
    };
    let editor_h = 5;

    let (constraints, slots) =
        build_constraints(mode, agent_h, has_notif, has_sugg, sugg_count, editor_h);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    match mode {
        LayoutMode::Chat | LayoutMode::PartialOverlay { .. } | LayoutMode::Approval => {
            // Slot A: Timeline
            app.timeline.render(frame, chunks[slots.timeline_or_full]);
        }
        LayoutMode::FullOverlay { mode: overlay_mode } => {
            // Slot A: Full Panel (replaces all middle slots)
            render_full_panel(frame, app, chunks[slots.timeline_or_full], overlay_mode);
            // Slot E: BottomBar
            BottomBar::render(frame, chunks[slots.bottom_bar], app);
            return;
        }
    }

    // Slot B: AgentPanel
    if let Some(idx) = slots.agent_panel {
        AgentPanel::render(frame, chunks[idx], app);
    }

    // Slot C: NotificationRow (conditional)
    if let Some(idx) = slots.notification_row {
        render_notification_row(frame, chunks[idx], app);
    }

    // Slot D': Completion suggestions (layout slot, not floater)
    if let Some(idx) = slots.suggestions {
        render_suggestions(frame, app, chunks[idx]);
    }

    // Slot D: Editor, Partial Panel, or Approval Panel
    match mode {
        LayoutMode::Chat => {
            if let Some(idx) = slots.editor {
                render_editor(frame, app, chunks[idx]);
            }
        }
        LayoutMode::PartialOverlay { mode: overlay_mode } => {
            if let Some(idx) = slots.partial_or_approval {
                render_partial_panel(frame, app, chunks[idx], overlay_mode);
            }
        }
        LayoutMode::Approval => {
            if let Some(idx) = slots.partial_or_approval {
                app.approvals.render(frame, chunks[idx]);
            }
            if let Some(idx) = slots.editor {
                render_editor(frame, app, chunks[idx]);
            }
        }
        LayoutMode::FullOverlay { .. } => unreachable!(),
    }

    // Slot E: BottomBar (always last)
    BottomBar::render(frame, chunks[slots.bottom_bar], app);
}

// ── Slot renderers ───────────────────────────────────────────────────────────

fn render_full_panel(frame: &mut Frame<'_>, app: &AppState, area: Rect, mode: AppMode) {
    match mode {
        AppMode::Help => HelpPanel::render(frame, area),
        AppMode::Sessions => app
            .sessions
            .render(frame, area, &app.filter_text, app.session_id()),
        AppMode::Tree => app.tree.render(frame, area, &app.filter_text),
        AppMode::Status => StatusPanel::render(frame, area, app, &app.timeline, &app.approvals),
        _ => {}
    }
}

fn render_partial_panel(frame: &mut Frame<'_>, app: &AppState, area: Rect, mode: AppMode) {
    match mode {
        AppMode::Commands => app.commands.render(frame, area, &app.filter_text),
        AppMode::Models => app.models.render(
            frame,
            area,
            &app.filter_text,
            app.initial_options.model_id.as_deref(),
        ),
        AppMode::Settings => app.settings.render(frame, area, &app.filter_text),
        _ => {}
    }
}

fn render_editor(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let input_title = if app.editor.text().starts_with('/') {
        "input command"
    } else {
        "input"
    };
    let widget = Paragraph::new(app.editor.text())
        .block(Block::default().borders(Borders::ALL).title(input_title))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);

    if app.mode == AppMode::Chat {
        let (row, col) = app.editor.cursor_line_col();
        let cursor_x = area.x + 1 + col.min(area.width.saturating_sub(2));
        let cursor_y = area.y + 1 + row.min(area.height.saturating_sub(2));
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_notification_row(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(notification) = app.notifications.visible() else {
        return;
    };
    let color = match notification.level {
        NotificationLevel::Info => Color::Cyan,
        NotificationLevel::Warning => Color::Yellow,
        NotificationLevel::Error => Color::Red,
    };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("│", Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(&notification.message, Style::default().fg(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

// ── Completion suggestions (layout slot, not floater) ────────────────────────

fn render_suggestions(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let items: Vec<ListItem<'_>> = app
        .completions
        .iter()
        .enumerate()
        .map(|(idx, completion)| suggestion_item(idx == app.selected_completion, completion))
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(
                "suggestions [{}/{}] | Tab accept | ↑↓ select",
                app.selected_completion + 1,
                app.completions.len()
            )),
    );
    frame.render_widget(list, area);
}

fn suggestion_item<'a>(selected: bool, completion: &'a Completion) -> ListItem<'a> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker}{}", completion.label), style),
        Span::styled(
            format!("  {}", completion.detail),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
}
