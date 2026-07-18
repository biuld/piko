//! Ordered attention queue: approvals before interactions (host order preserved).

use crate::state::{ClientState, LiveSession, PendingApproval, PendingInteraction};

/// Kind of blocking prompt in the front-of-queue projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionKind {
    Approval,
    Interaction,
}

/// One actionable prompt entry, preserving host order within each kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    pub kind: AttentionKind,
    pub id: String,
    pub agent_instance_id: String,
    pub summary: String,
    pub response_in_flight: bool,
}

/// Session-scoped prompt queue: all pending approvals (host order), then
/// all pending interactions (host order). Approvals always precede interactions.
pub fn prompt_queue(session: &LiveSession) -> Vec<AttentionItem> {
    let mut items =
        Vec::with_capacity(session.pending_approvals.len() + session.pending_interactions.len());
    for a in &session.pending_approvals {
        items.push(approval_item(a));
    }
    for i in &session.pending_interactions {
        items.push(interaction_item(i));
    }
    items
}

/// Front prompt for PromptCoordinator (None when idle).
pub fn front_prompt(session: &LiveSession) -> Option<AttentionItem> {
    prompt_queue(session).into_iter().next()
}

/// Look up the live pending approval by id.
pub fn find_approval<'a>(
    session: &'a LiveSession,
    approval_id: &str,
) -> Option<&'a PendingApproval> {
    session
        .pending_approvals
        .iter()
        .find(|a| a.approval_id == approval_id)
}

/// Look up the live pending interaction by id.
pub fn find_interaction<'a>(
    session: &'a LiveSession,
    interaction_id: &str,
) -> Option<&'a PendingInteraction> {
    session
        .pending_interactions
        .iter()
        .find(|i| i.interaction_id == interaction_id)
}

/// Convenience: front prompt from full client state when Live.
pub fn front_prompt_from_state(state: &ClientState) -> Option<AttentionItem> {
    state.live_session.as_ref().and_then(front_prompt)
}

/// Convenience: full queue from client state when Live.
pub fn prompt_queue_from_state(state: &ClientState) -> Vec<AttentionItem> {
    state
        .live_session
        .as_ref()
        .map(prompt_queue)
        .unwrap_or_default()
}

fn approval_item(a: &PendingApproval) -> AttentionItem {
    AttentionItem {
        kind: AttentionKind::Approval,
        id: a.approval_id.clone(),
        agent_instance_id: a.agent_instance_id.clone(),
        summary: format!("Approval: {}", a.tool_name),
        response_in_flight: a.response_in_flight,
    }
}

fn interaction_item(i: &PendingInteraction) -> AttentionItem {
    let label = i
        .questions
        .first()
        .map(|q| {
            if q.header.is_empty() {
                q.prompt.clone()
            } else {
                q.header.clone()
            }
        })
        .unwrap_or_else(|| "User interaction".into());
    AttentionItem {
        kind: AttentionKind::Interaction,
        id: i.interaction_id.clone(),
        agent_instance_id: i.agent_instance_id.clone(),
        summary: label,
        response_in_flight: i.response_in_flight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LiveSession;

    fn session_with(
        approvals: Vec<PendingApproval>,
        interactions: Vec<PendingInteraction>,
    ) -> LiveSession {
        LiveSession {
            session_id: "s1".into(),
            pending_approvals: approvals,
            pending_interactions: interactions,
            ..Default::default()
        }
    }

    fn approval(id: &str, tool: &str) -> PendingApproval {
        PendingApproval {
            approval_id: id.into(),
            agent_instance_id: "root".into(),
            tool_name: tool.into(),
            tool_args: serde_json::json!({}),
            response_in_flight: false,
        }
    }

    fn interaction(id: &str) -> PendingInteraction {
        PendingInteraction {
            interaction_id: id.into(),
            agent_instance_id: "root".into(),
            questions: vec![],
            require_confirm: false,
            response_in_flight: false,
        }
    }

    #[test]
    fn approvals_precede_interactions() {
        let session = session_with(vec![approval("a1", "exec")], vec![interaction("i1")]);
        let q = prompt_queue(&session);
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].kind, AttentionKind::Approval);
        assert_eq!(q[0].id, "a1");
        assert_eq!(q[1].kind, AttentionKind::Interaction);
        assert_eq!(q[1].id, "i1");
    }

    #[test]
    fn preserves_host_order_within_kind() {
        let session = session_with(
            vec![approval("a1", "a"), approval("a2", "b")],
            vec![interaction("i1"), interaction("i2")],
        );
        let q = prompt_queue(&session);
        let ids: Vec<_> = q.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["a1", "a2", "i1", "i2"]);
    }

    #[test]
    fn front_is_first_approval() {
        let session = session_with(
            vec![approval("a1", "exec"), approval("a2", "write")],
            vec![interaction("i1")],
        );
        let front = front_prompt(&session).unwrap();
        assert_eq!(front.id, "a1");
        assert_eq!(front.kind, AttentionKind::Approval);
    }

    #[test]
    fn flight_flag_preserved_in_queue() {
        let mut a = approval("a1", "exec");
        a.response_in_flight = true;
        let session = session_with(vec![a], vec![]);
        let front = front_prompt(&session).unwrap();
        assert!(front.response_in_flight);
    }
}
