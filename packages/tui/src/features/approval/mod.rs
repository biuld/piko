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
    /// F-13: operator-authored approval prompt (MCP approval templates);
    /// absent → the generic question is rendered.
    pub prompt: Option<String>,
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
                approval_question(approval),
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

/// Build the user-facing approval question. Shell-like tools (`bash`, and
/// `process start`, which runs a shell command under the hood — F-08) render
/// the inner command instead of raw JSON; everything else stays generic.
fn approval_question(approval: &PendingApproval) -> String {
    if let Some(prompt) = &approval.prompt {
        return prompt.clone();
    }
    let command = approval
        .args
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|c| !c.trim().is_empty());
    let action = approval
        .args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_shell_command = matches!(approval.tool_name.as_str(), "bash" | "process")
        && (approval.tool_name != "process" || action == "start")
        && command.is_some();
    if is_shell_command {
        format!("Run shell command `{}`?", command.expect("checked above"))
    } else {
        format!(
            "Run {} with args {}?",
            approval.tool_name,
            compact_json(&approval.args)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(tool_name: &str, args: serde_json::Value) -> PendingApproval {
        PendingApproval {
            id: "a1".into(),
            tool_name: tool_name.into(),
            args,
            prompt: None,
        }
    }

    #[test]
    fn shell_like_tools_render_the_inner_command() {
        let bash = approval(
            "bash",
            serde_json::json!({ "command": "cargo test -p tui" }),
        );
        assert_eq!(
            approval_question(&bash),
            "Run shell command `cargo test -p tui`?"
        );

        let start = approval(
            "process",
            serde_json::json!({ "action": "start", "command": "npm install" }),
        );
        assert_eq!(
            approval_question(&start),
            "Run shell command `npm install`?"
        );
    }

    #[test]
    fn non_shell_actions_stay_generic() {
        let write = approval(
            "process",
            serde_json::json!({ "action": "write", "processId": "proc-1" }),
        );
        assert!(approval_question(&write).starts_with("Run process with args"));

        let read = approval("read", serde_json::json!({ "path": "Cargo.toml" }));
        assert!(approval_question(&read).starts_with("Run read with args"));
    }

    #[test]
    fn operator_prompt_replaces_the_generic_question() {
        let templated = PendingApproval {
            id: "a2".into(),
            tool_name: "create_issue".into(),
            args: serde_json::json!({ "title": "x" }),
            prompt: Some("This creates a GitHub issue in the configured repository.".into()),
        };
        assert_eq!(
            approval_question(&templated),
            "This creates a GitHub issue in the configured repository."
        );
    }
}
