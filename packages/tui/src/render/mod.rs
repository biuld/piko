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
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::{
    app::{AppMode, AppState},
    features::{
        agent_status::AgentPanel, bottom_bar::BottomBar, editor::Completion, help::HelpPanel,
        notifications::NotificationLevel, status::StatusPanel,
    },
    layout::{
        LayoutMode, agent_panel_height, build_constraints, has_visible_notification,
        has_visible_suggestions,
    },
};

/// Main render entry point.
pub fn render(frame: &mut Frame<'_>, app: &mut AppState) {
    let area = frame.area();
    let mode = LayoutMode::from_app(app);
    let agent_h = agent_panel_height(app);
    let has_notif = has_visible_notification(app);
    let has_sugg = has_visible_suggestions(app);
    let sugg_count = if has_sugg { app.completions.len() } else { 0 };
    let editor_h = app
        .editor
        .visible_height(&app.tui_config.editor, area.width);

    let (constraints, slots) =
        build_constraints(mode, agent_h, has_notif, has_sugg, sugg_count, editor_h);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    match mode {
        LayoutMode::Chat | LayoutMode::PartialOverlay { .. } | LayoutMode::Approval => {
            // Slot A: Timeline
            app.timeline
                .render(frame, chunks[slots.timeline_or_full], &app.theme);
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
                app.approvals.render(frame, chunks[idx], &app.theme);
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
        AppMode::Help => HelpPanel::render(frame, area, &app.theme, &app.command_catalog),
        AppMode::Sessions => {
            app.sessions
                .render(frame, area, &app.filter_text, app.session_id(), &app.theme)
        }
        AppMode::Tree => app.tree.render(frame, area, &app.filter_text, &app.theme),
        AppMode::Status => StatusPanel::render(frame, area, app, &app.timeline, &app.approvals),
        _ => {}
    }
}

fn render_partial_panel(frame: &mut Frame<'_>, app: &AppState, area: Rect, mode: AppMode) {
    match mode {
        AppMode::Commands => app
            .commands
            .render(frame, area, &app.filter_text, &app.theme),
        AppMode::Models => app.models.render(
            frame,
            area,
            &app.filter_text,
            app.initial_options.model_id.as_deref(),
            &app.theme,
        ),
        AppMode::Settings => app
            .settings
            .render(frame, area, &app.filter_text, &app.theme),
        _ => {}
    }
}

fn render_editor(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    // pi-style: editor border color reflects thinking level.
    // Only top + bottom borders (no left/right).
    let border_color = app.theme.border_muted;
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(border_color));
    app.editor.render(frame, area, block);

    if app.mode == AppMode::Chat && app.timeline.is_at_latest() {
        let visible_rows = area.height.saturating_sub(2).max(1);
        let (row, col) = app.editor.cursor_line_col(area.width, visible_rows);
        let cursor_x = area.x + col.min(area.width.saturating_sub(1));
        let cursor_y = area.y + 1 + row.min(visible_rows.saturating_sub(1));
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_notification_row(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(notification) = app.notifications.visible() else {
        return;
    };
    let color = match notification.level {
        NotificationLevel::Info => app.theme.info,
        NotificationLevel::Warning => app.theme.warning,
        NotificationLevel::Error => app.theme.error,
    };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(&notification.message, Style::default().fg(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

// ── Completion suggestions (layout slot, not floater) ────────────────────────

fn render_suggestions(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let items: Vec<ListItem<'_>> = if app.completions.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "  no matches",
            Style::default().fg(app.theme.dim),
        )]))]
    } else {
        app.completions
            .iter()
            .enumerate()
            .map(|(idx, completion)| {
                suggestion_item(
                    idx == app.selected_completion,
                    completion,
                    app.theme.accent,
                    app.theme.dim,
                )
            })
            .collect()
    };
    let selected = if app.completions.is_empty() {
        0
    } else {
        app.selected_completion + 1
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.theme.border_muted))
            .title(format!(
                "suggestions [{}/{}] | Tab accept | ↑↓ select",
                selected,
                app.completions.len()
            )),
    );
    frame.render_widget(list, area);
}

fn suggestion_item<'a>(
    selected: bool,
    completion: &'a Completion,
    accent: ratatui::style::Color,
    dim: ratatui::style::Color,
) -> ListItem<'a> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker}{}", completion.label), style),
        Span::styled(format!("  {}", completion.detail), Style::default().fg(dim)),
    ]))
}
