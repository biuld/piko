use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::theme::Theme;

use crate::text::compact_json;

/// A single pending tool-approval request.
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// Approval state: a queue of pending requests.
pub struct ApprovalPanel {
    pub pending: VecDeque<PendingApproval>,
}

impl ApprovalPanel {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn push(&mut self, approval: PendingApproval) {
        self.pending.push_back(approval);
    }

    pub fn resolve(&mut self, id: &str) {
        self.pending.retain(|a| a.id != id);
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn front(&self) -> Option<&PendingApproval> {
        self.pending.front()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Render the approval popup if there is a pending request.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let Some(approval) = self.pending.front() else {
            return;
        };
        frame.render_widget(Clear, area);
        let body = format!(
            "Approval requested\n\nTool: {}\nArgs: {}\n\nCtrl-A accept once\nCtrl-S accept for session\nCtrl-W accept for workspace\nCtrl-D decline",
            approval.tool_name,
            compact_json(&approval.args)
        );
        let widget = Paragraph::new(body)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.warning))
                    .title("approval"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(widget, area);
    }

    /// Render informational label when there are no pending approvals.
    #[allow(dead_code)]
    pub fn render_count_label(count: usize, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if count > 0 {
            let label = format!(" {count} pending approvals ");
            let width = label.len() as u16 + 2;
            let x = area.x + area.width.saturating_sub(width + 2);
            let popup = Rect {
                x,
                y: area.y + 1,
                width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(theme.warning),
                ))),
                popup,
            );
        }
    }
}
