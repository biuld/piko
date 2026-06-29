use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::text::compact_json;

/// A single pending tool-approval request.
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// Approval state: a queue of pending requests.
pub struct ApprovalOverlay {
    pub pending: VecDeque<PendingApproval>,
}

impl ApprovalOverlay {
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
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
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
            .block(Block::default().borders(Borders::ALL).title("approval"))
            .wrap(Wrap { trim: true });
        frame.render_widget(widget, area);
    }

    /// Returns the status-bar hint line for when approvals are pending.
    pub fn help_hint() -> &'static str {
        "Approval: Ctrl-A once | Ctrl-S session | Ctrl-W workspace | Ctrl-D decline | Ctrl-L clear notes | Ctrl-Q quit"
    }

    /// Render informational label when there are no pending approvals.
    #[allow(dead_code)]
    pub fn render_count_label(count: usize, frame: &mut Frame<'_>, area: Rect) {
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
                    Style::default().fg(Color::Yellow),
                ))),
                popup,
            );
        }
    }
}
