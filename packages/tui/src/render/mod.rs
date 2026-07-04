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
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::{AppMode, AppState},
    features::{
        agent_status::AgentPanel, bottom_bar::BottomBar, help::HelpPanel,
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
    let sugg_count = if has_sugg {
        app.editor.auto_complete.len()
    } else {
        0
    };
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
        LayoutMode::Chat | LayoutMode::PartialOverlay { .. } => {
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
        app.editor
            .auto_complete
            .render(frame, chunks[idx], &app.theme);
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
        AppMode::Tree => app
            .tree
            .render(frame, area, &app.filter_text, None, &app.theme),
        AppMode::SummaryPrompt => app.tree.render(
            frame,
            area,
            &app.filter_text,
            app.summary_prompt.as_ref(),
            &app.theme,
        ),
        AppMode::Status => StatusPanel::render(frame, area, app, &app.timeline, &app.approvals),
        _ => {}
    }
}

fn render_partial_panel(frame: &mut Frame<'_>, app: &AppState, area: Rect, mode: AppMode) {
    match mode {
        AppMode::Models => app.models.render(
            frame,
            area,
            &app.filter_text,
            app.active_model_id.as_deref(),
            &app.theme,
        ),
        AppMode::Settings => app
            .settings
            .render(frame, area, &app.filter_text, app, &app.theme),
        AppMode::Approval => app.approvals.render(frame, area, &app.theme),
        AppMode::ToolInteraction => app.interactions.render(frame, area, &app.theme),
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

    if app.mode == AppMode::Chat {
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
