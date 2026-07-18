//! Composer enablement and target projection.

use piko_client_core::ClientState;
use piko_protocol::TurnStatus;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposerViewModel {
    pub can_send: bool,
    pub show_stop: bool,
    pub target_label: String,
    pub placeholder: String,
    pub model_label: String,
    pub thinking_label: String,
    pub can_cycle_model: bool,
    pub can_cycle_thinking: bool,
}

pub fn derive_composer(state: &ClientState) -> ComposerViewModel {
    let model_label = match (
        state.model.provider.as_deref(),
        state.model.model_id.as_deref(),
    ) {
        (Some(p), Some(m)) => format!("{p}/{m}"),
        (None, Some(m)) => m.to_string(),
        _ => "model".into(),
    };
    let thinking_label = state
        .model
        .thinking_level
        .clone()
        .unwrap_or_else(|| "off".into());
    let model_update_pending = state
        .pending_commands
        .values()
        .any(|op| matches!(op, piko_client_core::state::PendingOp::SetModel { .. }));
    let thinking_update_pending = state.pending_commands.values().any(|op| {
        matches!(
            op,
            piko_client_core::state::PendingOp::SetThinkingLevel { .. }
        )
    });
    let can_cycle_model = !state.model.providers.is_empty() && !model_update_pending;
    let can_cycle_thinking = !thinking_update_pending;

    if !state.is_live() {
        let idle = matches!(
            state.session_phase,
            piko_client_core::SessionPhase::IdleNoSession
        );
        return ComposerViewModel {
            can_send: idle,
            show_stop: false,
            target_label: "—".into(),
            placeholder: if idle {
                "Message to start a new session…".into()
            } else {
                "Waiting for session…".into()
            },
            model_label,
            thinking_label,
            can_cycle_model,
            can_cycle_thinking,
        };
    }

    let session = state.live_session.as_ref().unwrap();
    let Some(agent_id) = session.selected_agent.as_ref() else {
        return ComposerViewModel {
            can_send: false,
            show_stop: false,
            target_label: "—".into(),
            placeholder: "Select an agent…".into(),
            model_label,
            thinking_label,
            can_cycle_model,
            can_cycle_thinking,
        };
    };

    let name = session
        .agents
        .iter()
        .find(|a| &a.agent_instance_id == agent_id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| agent_id.clone());

    let show_stop = session.active_turns.iter().any(|t| {
        &t.agent_instance_id == agent_id
            && matches!(
                t.status,
                TurnStatus::Queued
                    | TurnStatus::Running
                    | TurnStatus::WaitingForApproval
                    | TurnStatus::Cancelling
            )
    });

    ComposerViewModel {
        // Send stays available while a turn runs (host may queue).
        can_send: true,
        show_stop,
        target_label: name,
        placeholder: "Message… (Enter to send, Shift+Enter for newline)".into(),
        model_label,
        thinking_label,
        can_cycle_model,
        can_cycle_thinking,
    }
}
