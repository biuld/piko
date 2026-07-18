//! Host-driven prompt front projection for the GUI coordinator.

use piko_client_core::{
    AttentionItem, AttentionKind, ClientState, front_prompt_from_state, prompt_queue_from_state,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Approval,
    Interaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFront {
    pub kind: PromptKind,
    pub id: String,
    pub agent_instance_id: String,
    pub remaining: usize,
    pub response_in_flight: bool,
    pub summary: String,
}

pub fn derive_prompt_front(state: &ClientState) -> Option<PromptFront> {
    let item = front_prompt_from_state(state)?;
    let remaining = prompt_queue_from_state(state).len();
    Some(from_attention(item, remaining))
}

pub fn prompt_fingerprint(front: Option<&PromptFront>) -> Option<String> {
    front.map(|f| format!("{:?}:{}", f.kind, f.id))
}

fn from_attention(item: AttentionItem, remaining: usize) -> PromptFront {
    let kind = match item.kind {
        AttentionKind::Approval => PromptKind::Approval,
        AttentionKind::Interaction => PromptKind::Interaction,
    };
    PromptFront {
        kind,
        id: item.id,
        agent_instance_id: item.agent_instance_id,
        remaining,
        response_in_flight: item.response_in_flight,
        summary: item.summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::ClientState;
    use piko_client_core::state::{LiveSession, PendingApproval, PendingInteraction, SessionPhase};

    #[test]
    fn approval_front_before_interaction() {
        let mut state = ClientState::default();
        state.session_phase = SessionPhase::Live;
        state.live_session = Some(LiveSession {
            session_id: "s1".into(),
            pending_approvals: vec![PendingApproval {
                approval_id: "a1".into(),
                agent_instance_id: "root".into(),
                tool_name: "exec".into(),
                tool_args: serde_json::json!({}),
                response_in_flight: false,
            }],
            pending_interactions: vec![PendingInteraction {
                interaction_id: "i1".into(),
                agent_instance_id: "root".into(),
                questions: vec![],
                require_confirm: false,
                response_in_flight: false,
            }],
            ..Default::default()
        });
        let front = derive_prompt_front(&state).unwrap();
        assert_eq!(front.kind, PromptKind::Approval);
        assert_eq!(front.id, "a1");
        assert_eq!(front.remaining, 2);
    }
}
