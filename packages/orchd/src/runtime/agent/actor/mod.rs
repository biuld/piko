use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

mod delivery;
mod input;
mod run_protocol;

use piko_comms::contracts::{
    AgentCommands, AgentRunReport as AgentRunReportContract, AgentRunStarted,
    AgentSnapshot as AgentSnapshotContract,
};
use piko_comms::{LatestSender, MailboxReceiver, MailboxSender, ReplySender};
use piko_orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::{
    AgentActivity, AgentCancelReceipt, AgentDurableCommand, AgentInboxItem, AgentInboxSnapshot,
    AgentInputDelivery, AgentInputReceipt, AgentInstanceIdentity, AgentInstanceLifecycle,
    AgentLifecycleReceipt, AgentMailboxEvent, AgentRunReport, AgentSnapshot, ConversationContext,
    ExecutionConfig, InputDisposition, SendAgentInputRequest, StartExecutionRequest,
    SteerExecutionRequest,
};

use super::mailbox::{AgentCommand, DetachedReportTarget};
use super::scope::SessionAgentScope;
use crate::runtime::execution::{AgentExecutionRuntime, ExecutionTerminal};
use crate::runtime::reliability::{
    ActorCommandScope, DetachedDeliveryResult, DetachedDeliveryScope, ExecutionHandoffLease,
    RunCancellation, RunStartupScope, StartedRunFailure, TerminalCommitResult, TerminalCommitScope,
};
use crate::runtime::utils::now_ms;

/// Fixed cap on the durable follow-up queue (F-01 / D-01). Exceeding it
/// returns overload; cancelling a queued input frees its slot.
const MAX_QUEUED_FOLLOW_UPS: usize = 64;

/// Long-lived serialization boundary for one AgentInstance.
pub struct AgentActor {
    identity: AgentInstanceIdentity,
    spec: piko_protocol::AgentSpec,
    lifecycle: AgentInstanceLifecycle,
    transcript: Vec<piko_protocol::Message>,
    head_message_id: Option<String>,
    inbox: Vec<AgentInboxItem>,
    follow_ups: VecDeque<QueuedRuntimeInput>,
    input_requests: HashMap<
        String,
        (
            SendAgentInputRequest,
            piko_protocol::AgentInput,
            Option<AcceptedAgentInput>,
        ),
    >,
    run_state: AgentRunState,
    latest_report: Option<AgentRunReport>,
    completed_executions: HashMap<String, AgentRunReport>,
    execution_waiters: HashMap<
        String,
        Vec<ReplySender<AgentRunReportContract, Result<AgentRunReport, AgentApiError>>>,
    >,
    detached_reports: HashMap<String, Vec<DetachedReportTarget>>,
    scope: std::sync::Weak<SessionAgentScope>,
    recovered_detached_deliveries: Vec<piko_orchd_api::RecoveredDetachedDelivery>,
    generation: u64,
    commit: Arc<dyn AgentCommitPort>,
    execution: Arc<AgentExecutionRuntime>,
    command_tx: MailboxSender<AgentCommands, AgentCommand>,
    mailbox: MailboxReceiver<AgentCommands, AgentCommand>,
    snapshot_tx: LatestSender<AgentSnapshotContract, AgentSnapshot>,
    run_cancellation: Arc<RunCancellation>,
    current_run_cancellation_generation: Option<u64>,
    /// Parent span for the next agent run, captured from the turn/tool
    /// context and consumed by `start_execution_from`.
    pending_run_parent: Option<tracing::Span>,
}

struct QueuedRuntimeInput {
    input: piko_protocol::AgentInput,
    request: SendAgentInputRequest,
    completion: Option<QueuedCompletion>,
    /// Parent span captured when the follow-up was queued.
    parent: tracing::Span,
}

enum QueuedCompletion {
    Waiter {
        started: ReplySender<AgentRunStarted, ()>,
        report: ReplySender<AgentRunReportContract, Result<AgentRunReport, AgentApiError>>,
    },
    Detached(DetachedReportTarget),
}

#[derive(Clone)]
struct AcceptedAgentInput {
    receipt: AgentInputReceipt,
    internal_execution_id: String,
}

enum AgentRunState {
    Idle,
    Starting {
        execution_id: String,
        root_input_id: String,
        run_id: String,
    },
    Running {
        execution_id: String,
        root_input_id: String,
        run_id: String,
    },
    Finalizing(TerminalCommitScope),
}

fn internal_execution_id(identity: &AgentInstanceIdentity, request_id: &str) -> String {
    piko_orchd_api::stable_internal_id(
        "exec",
        &[
            &identity.session_id,
            &identity.agent_instance_id,
            request_id,
        ],
    )
}

impl AgentRunState {
    fn execution_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Starting { execution_id, .. } | Self::Running { execution_id, .. } => {
                Some(execution_id)
            }
            Self::Finalizing(terminal) => Some(terminal.execution_id()),
        }
    }

    fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    fn active_root(&self) -> Option<(&str, &str)> {
        match self {
            Self::Starting {
                root_input_id,
                run_id,
                ..
            }
            | Self::Running {
                root_input_id,
                run_id,
                ..
            } => Some((root_input_id, run_id)),
            Self::Idle | Self::Finalizing(_) => None,
        }
    }
}

mod actor_impl;
