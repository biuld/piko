use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::features::approval::ApprovalPanel;
use crate::features::timeline::Timeline;
use crate::{
    app::{QueueStatus, ToolStatus},
    features::notifications::NotificationCenter,
    theme::Theme,
};

use super::centered_rect;

/// Status panel: read-only diagnostic panel.
pub struct StatusPanel;

pub struct StatusPanelView<'a> {
    pub session_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub queue: &'a QueueStatus,
    pub notifications: &'a NotificationCenter,
    pub theme: &'a Theme,
}

impl StatusPanel {
    pub fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        view: StatusPanelView<'_>,
        timeline: &Timeline,
        approvals: &ApprovalPanel,
    ) {
        let popup = centered_rect(76, 58, area);
        frame.render_widget(Clear, popup);

        let running = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Running)
            .count();
        let completed = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Completed)
            .count();
        let failed = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Failed)
            .count();

        let session = view.session_id.unwrap_or("none");
        let turn = view.turn_id.unwrap_or("none");
        let approvals_len = approvals.len().to_string();
        let queue_str = format!(
            "steer={} follow_up={} next_turn={}",
            view.queue.steer_count, view.queue.follow_up_count, view.queue.next_turn_count
        );
        let tools_str = format!(
            "{} total, {running} running, {completed} completed, {failed} failed",
            timeline.tool_calls.len()
        );
        let notifications_len = view.notifications.items().len().to_string();

        let accent = view.theme.accent;
        let mut lines = vec![
            kv("session ", session, accent),
            kv("active turn ", turn, accent),
            kv("queue ", &queue_str, accent),
            kv("approvals ", &approvals_len, accent),
            kv("tools ", &tools_str, accent),
            kv("notifications ", &notifications_len, accent),
        ];

        if let Some(preview) = &view.queue.steer_preview {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "steer preview",
                Style::default().fg(view.theme.warning),
            )));
            lines.push(Line::from(preview.as_str()));
        }
        if let Some(preview) = &view.queue.follow_up_preview {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "follow-up preview",
                Style::default().fg(view.theme.warning),
            )));
            lines.push(Line::from(preview.as_str()));
        }

        let widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(view.theme.border_muted))
                    .title("status | Esc close"),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(widget, popup);
    }
}

fn kv<'a>(key: &'a str, value: &'a str, accent: ratatui::style::Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, Style::default().fg(accent)),
        Span::raw(value.to_string()),
    ])
}
