//! Live work, tool, queue, and prompt event projection.

use crate::state::ClientState;

use super::is_live_session_event;

pub(super) fn handle_approval_event(state: &mut ClientState, event: piko_protocol::ApprovalEvent) {
    match event {
        piko_protocol::ApprovalEvent::Requested {
            session_id,
            agent_instance_id,
            approval_id,
            tool_name,
            tool_args,
            prompt,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session
                && !session
                    .pending_approvals
                    .iter()
                    .any(|a| a.approval_id == approval_id)
            {
                session
                    .pending_approvals
                    .push(crate::state::PendingApproval {
                        approval_id,
                        agent_instance_id: agent_instance_id.clone(),
                        tool_name,
                        tool_args,
                        prompt,
                        response_in_flight: false,
                    });
                crate::foreground::refresh_prompt_blocking(
                    &mut session.agents,
                    &session.agent_work,
                    &session.pending_approvals,
                    &session.pending_interactions,
                    &agent_instance_id,
                );
            }
        }
        piko_protocol::ApprovalEvent::Resolved {
            session_id,
            approval_id,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                let agent_instance_id = session
                    .pending_approvals
                    .iter()
                    .find(|a| a.approval_id == approval_id)
                    .map(|a| a.agent_instance_id.clone());
                session
                    .pending_approvals
                    .retain(|a| a.approval_id != approval_id);
                if let Some(agent_instance_id) = agent_instance_id {
                    crate::foreground::refresh_prompt_blocking(
                        &mut session.agents,
                        &session.agent_work,
                        &session.pending_approvals,
                        &session.pending_interactions,
                        &agent_instance_id,
                    );
                }
            }
        }
    }
}

pub(super) fn handle_interaction_event(
    state: &mut ClientState,
    event: piko_protocol::InteractionEvent,
) {
    match event {
        piko_protocol::InteractionEvent::Requested {
            session_id,
            agent_instance_id,
            interaction_id,
            questions,
            require_confirm,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session
                && !session
                    .pending_interactions
                    .iter()
                    .any(|i| i.interaction_id == interaction_id)
            {
                session
                    .pending_interactions
                    .push(crate::state::PendingInteraction {
                        interaction_id,
                        agent_instance_id: agent_instance_id.clone(),
                        questions,
                        require_confirm,
                        response_in_flight: false,
                    });
                crate::foreground::refresh_prompt_blocking(
                    &mut session.agents,
                    &session.agent_work,
                    &session.pending_approvals,
                    &session.pending_interactions,
                    &agent_instance_id,
                );
            }
        }
        piko_protocol::InteractionEvent::Resolved {
            session_id,
            interaction_id,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                let agent_instance_id = session
                    .pending_interactions
                    .iter()
                    .find(|i| i.interaction_id == interaction_id)
                    .map(|i| i.agent_instance_id.clone());
                session
                    .pending_interactions
                    .retain(|i| i.interaction_id != interaction_id);
                if let Some(agent_instance_id) = agent_instance_id {
                    crate::foreground::refresh_prompt_blocking(
                        &mut session.agents,
                        &session.agent_work,
                        &session.pending_approvals,
                        &session.pending_interactions,
                        &agent_instance_id,
                    );
                }
            }
        }
    }
}

pub(super) fn handle_usage_event(state: &mut ClientState, event: piko_protocol::UsageEvent) {
    let piko_protocol::UsageEvent::Updated {
        session_id,
        used,
        size,
        cumulative,
        ..
    } = event;

    if !is_live_session_event(state, &session_id) {
        return;
    }

    if let Some(window) = size.filter(|w| *w > 0) {
        state.model.context_window = Some(window);
    }

    let Some(session) = &mut state.live_session else {
        return;
    };
    if used > 0 {
        session.last_context_tokens = Some(used);
    }
    if let Some(cumulative) = cumulative {
        session.cumulative_usage = Some(cumulative);
    }
}

pub(super) fn handle_stream_item(
    state: &mut ClientState,
    patch: piko_protocol::StreamItemPatch,
    ctx: &mut crate::update::UpdateContext<'_>,
    effects: &mut Vec<crate::effect::ClientEffect>,
) {
    let Some(session_id) = patch.session_id.as_deref() else {
        return;
    };
    if !is_live_session_event(state, session_id) {
        return;
    }
    let Some(agent_instance_id) = patch.agent_instance_id.clone() else {
        return;
    };
    let outcome = {
        let Some(session) = &mut state.live_session else {
            return;
        };
        let timeline = session.timeline_mut(&agent_instance_id);
        timeline.apply_stream_item(&patch)
    };
    if outcome == crate::timeline::ApplyOutcome::Inconsistent {
        super::request_refresh(state, ctx, effects);
    }
}
