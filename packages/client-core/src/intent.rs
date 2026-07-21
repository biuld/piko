//! Frontend-neutral product operations.

use piko_protocol::{
    AgentInstanceId, ApprovalDecision, ApprovalId, InteractionId, SessionId, SessionListScope,
    ThinkingLevel, UserInteractionResponse,
};

/// Product intents emitted by a frontend adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientIntent {
    DiscoverSessions {
        scope: SessionListScope,
        cwd: Option<String>,
    },
    OpenSession {
        session_id: SessionId,
        session_path: Option<String>,
    },
    CreateSession {
        cwd: String,
    },
    RefreshSession,
    SelectAgent {
        agent_instance_id: AgentInstanceId,
    },
    SubmitTurn {
        text: String,
    },
    CancelTurn,
    RespondApproval {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        note: Option<String>,
    },
    RespondInteraction {
        interaction_id: InteractionId,
        response: UserInteractionResponse,
    },
    DeleteSession {
        session_id: SessionId,
    },
    RenameSession {
        session_id: SessionId,
        name: String,
    },
    NavigateSession {
        entry_id: String,
        summarize: bool,
        custom_instructions: Option<String>,
    },
    /// Request host model catalog (`ModelList` → `ModelListed`).
    ListModels,
    /// Pull current host defaults via a no-op `ConfigUpdate` (`ModelEvent`).
    ///
    /// Frontends call this at bootstrap so Composer can show `default-model` /
    /// `default-thinking-level` before the user cycles either value.
    SyncModelConfig,
    /// Persist default model via `ConfigUpdate` (correlated by `ModelEvent`).
    SetModel {
        provider: String,
        model_id: String,
    },
    /// Persist default thinking level via `ConfigUpdate` (correlated by `ModelEvent`).
    SetThinkingLevel {
        level: ThinkingLevel,
    },
}
