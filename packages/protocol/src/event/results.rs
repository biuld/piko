use serde::{Deserialize, Serialize};

use super::*;
use crate::HostCommandDescriptor;
use crate::command::OAuthLoginMode;
use crate::model::ProviderInfo;
use crate::session::SessionTreeEntry;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandResult {
    Empty,
    AgentInputSubmitted {
        receipt: crate::AgentInputReceipt,
        timestamp: i64,
    },
    AgentInputCancelled {
        receipt: crate::AgentInputCancelReceipt,
        timestamp: i64,
    },
    AgentInterrupted {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        accepted: bool,
        timestamp: i64,
    },
    AuthLoginStarted {
        login_id: String,
        provider: String,
        mode: OAuthLoginMode,
        timestamp: i64,
    },
    SessionCreated {
        session_id: SessionId,
        cwd: String,
        timestamp: i64,
    },
    /// Session identity only — visible view arrives via `SessionReconciled`.
    SessionOpened {
        session_id: SessionId,
        timestamp: i64,
    },
    SessionListed {
        sessions: Vec<SessionSummary>,
        timestamp: i64,
    },
    SessionNavigated {
        session_id: SessionId,
        old_leaf_id: Option<String>,
        new_leaf_id: Option<String>,
        selected_entry_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        editor_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary_entry: Option<SessionTreeEntry>,
        timestamp: i64,
    },
    ModelListed {
        providers: Vec<ProviderInfo>,
        timestamp: i64,
    },
    CommandCatalogListed {
        commands: Vec<HostCommandDescriptor>,
        timestamp: i64,
    },
    RolloutPaged {
        page: crate::RolloutPage,
        timestamp: i64,
    },
    TurnDiffGot {
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<TurnDiffEvent>,
        timestamp: i64,
    },
    /// Live set of external processes spawned by the `process` tool (F-08).
    ProcessListed {
        processes: Vec<crate::command::ProcessInfo>,
        timestamp: i64,
    },
    /// Result of terminating one process (F-08 client surface).
    ProcessStopped {
        process_id: String,
        /// False when no such process was running.
        stopped: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
        timestamp: i64,
    },
    /// MCP server connection status (F-13): one entry per configured server.
    McpStatusListed {
        servers: Vec<crate::command::McpServerInfo>,
        timestamp: i64,
    },
    AgentSpecListed {
        agents: Vec<crate::agents::AgentSpec>,
        timestamp: i64,
    },
    AgentListed {
        session_id: SessionId,
        agents: Vec<AgentInfo>,
        timestamp: i64,
    },
    AgentSubscribed {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        snapshot: AgentViewSnapshot,
        replay: Vec<SequencedServerMessage>,
        next_seq: u64,
    },
    ConfigEntry {
        namespace: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthFailureReason {
    Denied,
    Expired,
    Cancelled,
    Callback,
    Provider,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthEvent {
    LoginBrowser {
        login_id: String,
        provider: String,
        authorization_url: String,
    },
    LoginDeviceCode {
        login_id: String,
        provider: String,
        user_code: String,
        verification_uri: String,
    },
    LoginSuccess {
        #[serde(skip_serializing_if = "Option::is_none")]
        login_id: Option<String>,
        provider: String,
    },
    LoginFailed {
        login_id: String,
        provider: String,
        reason: AuthFailureReason,
        error: String,
    },
    LoggedOut {
        provider: String,
    },
}

impl From<AuthEvent> for ServerMessage {
    fn from(event: AuthEvent) -> Self {
        Self::Auth(event)
    }
}
