use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use piko_comms::MailboxReceiver;
use piko_comms::contracts::ExecutionCommands;
use piko_orchd_api::telemetry::ModelStepTelemetry;
use piko_orchd_api::{AgentApiError, CancelReceipt, InputDisposition};
use piko_protocol::execution::ExecutionInputReceipt;
use piko_protocol::execution::{
    ExecutionOutcome, ExecutionStatus, ModelStepOutcome, StartExecutionRequest,
    SteerExecutionRequest,
};
use piko_protocol::{Message, Usage};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::ExecutionIdentity;
use super::mailbox::ExecutionCommand;
use super::scope::SessionExecutionScope;
use super::services::ExecutionServices;
use super::state::ExecutionState;
use super::tool_batch;
use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::ToolCallItem;
use crate::domain::tools::definition::ToolExecutionMode;
use crate::domain::transcript::{TranscriptManager, TranscriptPolicy};
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::reliability::{ActorCommandScope, MessageCommitScope};
use crate::runtime::runtime_assistant_message_id;
use crate::runtime::step::StepDispatch;
use crate::runtime::tools::{build_tool_error, build_tool_result};
use piko_llmd::gateway::InferenceRequest;

#[derive(Debug, Clone)]
pub struct ExecutionRunResult {
    pub outcome: ExecutionOutcome,
    pub transcript: Vec<Message>,
    pub head_message_id: Option<String>,
}

pub struct ExecutionActor {
    identity: ExecutionIdentity,
    state: ExecutionState,
    mailbox: MailboxReceiver<ExecutionCommands, ExecutionCommand>,
    cancel: CancellationToken,
    ports: Arc<SessionExecutionScope>,
    services: ExecutionServices,
    request: StartExecutionRequest,
    tools: Vec<piko_protocol::ToolDef>,
    routes: HashMap<String, CatalogRoute>,
}

mod control;
mod run;
mod tools;

struct CompletedModelStep {
    model_step_id: String,
    step_index: u32,
    started_at: i64,
    finished_at: i64,
    outcome: ModelStepOutcome,
    failure: Option<String>,
    cancelled: bool,
    assistant_message: Message,
    tool_calls: Vec<ToolCallItem>,
    routes: HashMap<String, CatalogRoute>,
    message_id: String,
    model: ModelSpec,
    context_remaining: Option<u64>,
    /// True when this step was forced to answer a steered user message
    /// without tools (F-35 / ADR-021).
    respond_after_steer: bool,
}
