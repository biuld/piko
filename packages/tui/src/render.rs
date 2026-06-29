use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::{
    app::{AppMode, AppState, SurfaceInputPolicy, SurfacePlacement},
    input::completion::Completion,
    notification::NotificationLevel,
    surfaces::{
        approval::ApprovalOverlay, help::HelpOverlay, status::StatusOverlay,
        status_panel::StatusPanel,
    },
};

/// Top-level render function: performs fixed layout then delegates to surfaces.
pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    let active_mode = app.focus_manager.active_mode();
    let status_height = StatusPanel::height(app);

    // 1. Tool approval / pending approvals treated specially: status stays visible, editor is replaced.
    // If approval surface is active, layout shows: Header, Status, Approval, bottom-bar. (Timeline is hidden).
    if active_mode == AppMode::Approval || !app.approvals.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),             // Header
                Constraint::Length(status_height), // Status Panel
                Constraint::Min(6),                // Approval surface
                Constraint::Length(2),             // Help line / bottom bar
            ])
            .split(area);

        render_header(frame, app, chunks[0]);
        StatusPanel::render(frame, chunks[1], app);
        app.approvals.render(frame, chunks[2]);
        render_help_line(frame, app, chunks[3]);

        // Floating overlays (rendered on top)
        render_notifications(frame, app, area);
        return;
    }

    // 2. Otherwise, check if a full panel is active
    let full_panel_active = active_mode.placement() == Some(SurfacePlacement::Full);
    let has_capture_panel = active_mode.input_policy() == SurfaceInputPolicy::Capture;

    let timeline_panel_height = Constraint::Min(6);
    let status_slot_height = if has_capture_panel { 0 } else { status_height };
    let partial_panel_height = if active_mode.placement() == Some(SurfacePlacement::Partial) {
        10
    } else {
        0
    };
    let editor_slot_height = if has_capture_panel { 0 } else { 5 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                    // Header
            timeline_panel_height,                    // Timeline OR Full Panel
            Constraint::Length(status_slot_height),   // Status
            Constraint::Length(partial_panel_height), // Partial Panel
            Constraint::Length(editor_slot_height),   // Editor
            Constraint::Length(2),                    // Help line
        ])
        .split(area);

    render_header(frame, app, chunks[0]);

    if full_panel_active {
        match active_mode {
            AppMode::Help => HelpOverlay::render(frame, chunks[1]),
            AppMode::Sessions => app.sessions.render(frame, chunks[1]),
            AppMode::Tree => app.tree.render(frame, chunks[1]),
            AppMode::Status => {
                StatusOverlay::render(frame, chunks[1], app, &app.timeline, &app.approvals)
            }
            _ => {}
        }
    } else {
        app.timeline.render(frame, chunks[1]);
    }

    if status_slot_height > 0 {
        StatusPanel::render(frame, chunks[2], app);
    }

    if partial_panel_height > 0 {
        match active_mode {
            AppMode::Commands => app.commands.render(frame, chunks[3]),
            AppMode::Models => app.models.render(frame, chunks[3]),
            AppMode::Settings => app.settings.render(frame, chunks[3]),
            _ => {}
        }
    }

    if editor_slot_height > 0 {
        render_input(frame, app, chunks[4]);
    }

    render_help_line(frame, app, chunks[5]);

    // floating overlays (rendered after base so they appear on top)
    if editor_slot_height > 0 && status_slot_height > 0 {
        render_completions(frame, app, chunks[2]);
    }
    render_notifications(frame, app, area);
}

// ── layout sections ───────────────────────────────────────────────────────────

fn render_header(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let title = app
        .session_id()
        .map(|id| format!("piko hostd tui | {id}"))
        .unwrap_or_else(|| "piko hostd tui".to_string());
    let widget = Paragraph::new(app.status.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

fn render_input(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
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

fn render_help_line(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let text = if app.approvals.is_empty() {
        "Enter submit | Ctrl-K commands | Ctrl-N newline | PgUp/PgDn scroll | F1 help | F2 sessions | F3 models | /status | Ctrl-L clear notes | Ctrl-Q quit".to_string()
    } else {
        ApprovalOverlay::help_hint().to_string()
    };
    let widget = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(widget, area);
}

// ── floating overlays ─────────────────────────────────────────────────────────

fn render_notifications(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    if app.notifications.items().is_empty() {
        return;
    }
    let height = (app.notifications.items().len() as u16 + 2).min(7);
    let width = area.width.min(84);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y,
        width,
        height,
    };
    let items = app
        .notifications
        .items()
        .iter()
        .rev()
        .map(|notification| {
            let (label, color) = match notification.level {
                NotificationLevel::Info => ("info", Color::Cyan),
                NotificationLevel::Warning => ("warn", Color::Yellow),
                NotificationLevel::Error => ("error", Color::Red),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{label} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(notification.message.as_str()),
            ]))
        })
        .collect::<Vec<_>>();
    use ratatui::widgets::Clear;
    frame.render_widget(Clear, popup);
    let widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("notifications | Ctrl-L clear"),
    );
    frame.render_widget(widget, popup);
}

fn render_completions(frame: &mut Frame<'_>, app: &AppState, input_area: Rect) {
    if app.mode != AppMode::Chat || app.completions.is_empty() {
        return;
    }
    let height = (app.completions.len() as u16 + 2).min(8);
    let width = input_area.width.min(80);
    let y = input_area.y.saturating_sub(height);
    let area = Rect {
        x: input_area.x,
        y,
        width,
        height,
    };
    use ratatui::widgets::Clear;
    frame.render_widget(Clear, area);
    let items = app
        .completions
        .iter()
        .enumerate()
        .map(|(idx, completion)| completion_item(idx == app.selected_completion, completion))
        .collect::<Vec<_>>();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("suggestions | Tab accept | Up/Down select"),
    );
    frame.render_widget(list, area);
}

fn completion_item(selected: bool, completion: &Completion) -> ListItem<'_> {
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
