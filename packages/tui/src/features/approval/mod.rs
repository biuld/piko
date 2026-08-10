use std::collections::VecDeque;

use piko_protocol::ApprovalDecision;
use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use crate::app::{HitId, command::ApprovalAction};
use crate::navigation::SurfaceId;
use crate::theme::Theme;
use crate::ui::components::pane::PaneTitleAffix;
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

use crate::text::compact_json;
use crate::ui::components::choice_workflow::{ChoiceOption, ChoiceWorkflow, Question};

/// A single pending tool-approval request.
pub struct PendingApproval {
    pub id: String,
    /// Agent instance that requested the tool; used for F-22 foreground projection.
    pub agent_instance_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    /// F-13: operator-authored approval prompt (MCP approval templates);
    /// absent → the generic question is rendered.
    pub prompt: Option<String>,
    /// Durable choice selection for list navigation (default: Accept once).
    pub selected_idx: usize,
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

    pub const CHOICE_COUNT: usize = 5;

    pub fn select_next(&mut self) {
        if let Some(front) = self.pending.front_mut()
            && front.selected_idx + 1 < Self::CHOICE_COUNT
        {
            front.selected_idx += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if let Some(front) = self.pending.front_mut()
            && front.selected_idx > 0
        {
            front.selected_idx -= 1;
        }
    }

    pub fn select_choice(&mut self, idx: usize) {
        if let Some(front) = self.pending.front_mut()
            && idx < Self::CHOICE_COUNT
        {
            front.selected_idx = idx;
        }
    }

    pub fn selected_decision(&self) -> Option<ApprovalDecision> {
        self.pending
            .front()
            .map(|a| approval_decision(a.selected_idx))
    }

    /// Build the single-question approval workflow for the front request.
    pub(crate) fn workflow(&self) -> Option<ChoiceWorkflow> {
        let approval = self.pending.front()?;
        let mut question = Question::new(
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
        );
        question.selected_idx = approval.selected_idx.min(Self::CHOICE_COUNT - 1);
        Some(
            ChoiceWorkflow::new(vec![question], false).with_help(format!(
                "↑↓ select · Enter confirm · Esc decline · tool {}",
                approval.tool_name,
            )),
        )
    }

    /// Render the approval dock if there is a pending request.
    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        if let Some(workflow) = self.workflow() {
            let mut affixes = Vec::new();
            if let Some(a) = self.pending.front() {
                affixes.push(PaneTitleAffix::label(format!("tool: {}", a.tool_name)));
            }
            if self.pending.len() > 1 {
                affixes.push(PaneTitleAffix::selection(1, self.pending.len()));
            }
            workflow.render_in_pane(frame, area, theme, "Approval", affixes, interaction);
        }
    }
}

impl Component<HitId, Theme> for ApprovalPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx, InteractionState::default());
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx, interaction);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.workflow()
            .map(|workflow| workflow.component_regions_modal(area))
            .unwrap_or_default()
    }
}

impl SurfacePanel<SurfaceId, HitId, Theme> for ApprovalPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Approval
    }
}

impl PointerComponent<HitId> for ApprovalPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        if gesture != PointerGesture::Activate {
            return Vec::new();
        }
        let Some(HitId::Choice { choice, .. }) = hit.element else {
            return Vec::new();
        };
        self.select_choice(choice);
        vec![ApprovalAction::Respond(approval_decision(choice)).into()]
    }
}

fn approval_decision(choice: usize) -> ApprovalDecision {
    match choice {
        0 => ApprovalDecision::Accept,
        1 => ApprovalDecision::AcceptSession,
        2 => ApprovalDecision::AcceptWorkspace,
        3 => ApprovalDecision::AcceptPermanent,
        _ => ApprovalDecision::Decline,
    }
}

/// Build the user-facing approval question. Command approvals expose the
/// requested authority and justification instead of hiding them in raw JSON.
fn approval_question(approval: &PendingApproval) -> String {
    if let Some(prompt) = &approval.prompt {
        return prompt.clone();
    }
    let command = approval
        .args
        .get("cmd")
        .and_then(|v| v.as_str())
        .filter(|c| !c.trim().is_empty());
    let authority = approval
        .args
        .get("sandbox_permissions")
        .and_then(|v| v.as_str())
        .unwrap_or("use_default");
    if let ("exec_command", Some(command)) = (approval.tool_name.as_str(), command) {
        let justification = approval
            .args
            .get("justification")
            .and_then(|value| value.as_str());
        match justification {
            Some(reason) => format!(
                "Run command `{}` with authority `{authority}`? Reason: {reason}",
                command
            ),
            None => format!("Run command `{command}`?"),
        }
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
            agent_instance_id: "agent-1".into(),
            tool_name: tool_name.into(),
            args,
            prompt: None,
            selected_idx: 0,
        }
    }

    #[test]
    fn command_tool_renders_authority_and_justification() {
        let command = approval(
            "exec_command",
            serde_json::json!({
                "cmd": "cargo test -p tui",
                "sandbox_permissions": "require_escalated",
                "justification": "sandbox denied the first attempt"
            }),
        );
        assert_eq!(
            approval_question(&command),
            "Run command `cargo test -p tui` with authority `require_escalated`? Reason: sandbox denied the first attempt"
        );
    }

    #[test]
    fn non_shell_actions_stay_generic() {
        let write = approval("write_stdin", serde_json::json!({ "session_id": "proc-1" }));
        assert!(approval_question(&write).starts_with("Run write_stdin with args"));

        let read = approval("read", serde_json::json!({ "path": "Cargo.toml" }));
        assert!(approval_question(&read).starts_with("Run read with args"));
    }

    #[test]
    fn operator_prompt_replaces_the_generic_question() {
        let templated = PendingApproval {
            id: "a2".into(),
            agent_instance_id: "agent-1".into(),
            tool_name: "create_issue".into(),
            args: serde_json::json!({ "title": "x" }),
            prompt: Some("This creates a GitHub issue in the configured repository.".into()),
            selected_idx: 0,
        };
        assert_eq!(
            approval_question(&templated),
            "This creates a GitHub issue in the configured repository."
        );
    }

    #[test]
    fn selection_persists_across_workflow_rebuild() {
        let mut panel = ApprovalPanel::new();
        panel.push(approval("bash", serde_json::json!({})));
        panel.select_next();
        panel.select_next();
        assert_eq!(panel.front().unwrap().selected_idx, 2);
        let wf = panel.workflow().unwrap();
        assert_eq!(wf.questions[0].selected_idx, 2);
        assert_eq!(
            panel.selected_decision(),
            Some(ApprovalDecision::AcceptWorkspace)
        );
    }
}
