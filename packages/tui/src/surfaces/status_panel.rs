use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, QueueStatus},
    notification::{Notification, NotificationLevel},
};

/// Inline StatusPanel rendered between Timeline and Input.
/// Matches the TS TUI AgentPanel + Notification line spec.
pub struct StatusPanel;

impl StatusPanel {
    pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
        let mut lines = Vec::new();

        // 1. Agent Row (fallback agent "main")
        let is_running = app.active_turn_id().is_some();
        let has_queue = app.queue_status.steer_count > 0
            || app.queue_status.follow_up_count > 0
            || app.queue_status.next_turn_count > 0;

        if is_running || has_queue {
            lines.push(render_agent_row(
                is_running,
                &app.queue_status,
                app.spinner_frame,
            ));
        } else {
            lines.push(render_idle_agent_row());
        }

        // 2. Notification Line (only when idle and there's a notification)
        // Note: The spec says to show it when idle. For simplicity, we can always
        // show the latest notification if it's unread/unexpired, but we'll follow
        // the rule: if not running, and we have a notification, show it.
        if !is_running {
            if let Some(notification) = app.notifications.items().front() {
                lines.push(render_notification_row(notification));
            }
        }

        let widget = Paragraph::new(lines).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(widget, area);
    }

    /// Returns how many lines the status panel needs to render.
    pub fn height(app: &AppState) -> u16 {
        let is_running = app.active_turn_id().is_some();
        let has_queue = app.queue_status.steer_count > 0
            || app.queue_status.follow_up_count > 0
            || app.queue_status.next_turn_count > 0;

        let mut h = 1; // 1 for the top border
        if is_running || has_queue {
            h += 1;
        } else {
            h += 1; // Idle agent row
        }

        if !is_running && !app.notifications.items().is_empty() {
            h += 1;
        }

        h
    }
}

fn render_agent_row<'a>(is_running: bool, queue: &QueueStatus, frame_idx: usize) -> Line<'a> {
    let spinner = if is_running {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        frames[frame_idx % frames.len()]
    } else {
        "●"
    };

    let marker_style = if is_running {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let mut spans = vec![
        Span::raw(" "), // padding left
        Span::styled(spinner, marker_style),
        Span::raw("   "), // gap
        Span::styled("main", Style::default().add_modifier(Modifier::BOLD)),
    ];

    let total_queue = queue.steer_count + queue.follow_up_count + queue.next_turn_count;
    if total_queue > 0 {
        spans.push(Span::raw("    ")); // gap
        spans.push(Span::styled(
            format!("{} queued", total_queue),
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

fn render_idle_agent_row<'a>() -> Line<'a> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("●", Style::default().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("main", Style::default().add_modifier(Modifier::BOLD)),
    ])
}

fn render_notification_row<'a>(notification: &Notification) -> Line<'a> {
    let color = match notification.level {
        NotificationLevel::Info => Color::Cyan,
        NotificationLevel::Warning => Color::Yellow,
        NotificationLevel::Error => Color::Red,
    };

    Line::from(vec![
        Span::raw(" "),                                // padding left
        Span::styled("│", Style::default().fg(color)), // marker
        Span::raw("   "),                              // gap
        Span::styled(notification.message.clone(), Style::default().fg(color)),
    ])
}
