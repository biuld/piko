use std::collections::VecDeque;

use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

use crate::theme::Theme;

use crate::text::compact_json;
use crate::ui::components::interactive_workflow::{ChoiceOption, InteractiveWorkflow, Question};

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
        let workflow = InteractiveWorkflow::new(
            vec![Question::new(
                "Approval",
                format!(
                    "Run {} with args {}?",
                    approval.tool_name,
                    compact_json(&approval.args)
                ),
                vec![
                    ChoiceOption {
                        label: "Accept once".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                    ChoiceOption {
                        label: "Accept for session".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                    ChoiceOption {
                        label: "Accept for workspace".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                    ChoiceOption {
                        label: "Accept permanently".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                    ChoiceOption {
                        label: "Decline".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                ],
            )],
            false,
        );
        workflow.render(frame, area, theme);
        let help = Paragraph::new(format!(
            " Enter accept once · A session · W workspace · P permanent · Esc decline · tool {} ",
            approval.tool_name,
        ))
        .style(Style::default().fg(theme.muted));
        let y = area.y + area.height.saturating_sub(1);
        frame.render_widget(
            help,
            Rect::new(area.x.saturating_add(2), y, area.width.saturating_sub(4), 1),
        );
    }
}
