use std::pin::Pin;

use futures_core::Stream;
use llmd::gateway::{GatewayEvent, GatewayRequest};

use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelSpec;
use crate::domain::tasks::task::AgentTask;
use crate::domain::transcript::TranscriptManager;
use crate::ports::tool_provider::ToolDiscoveryContext;
use crate::runtime::events::TaskEventEmitter;
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::events::task_lifecycle::TaskLifecycleConsumer;
use crate::runtime::runtime_assistant_message_id;
use crate::runtime::step::StepDispatch;
use crate::runtime::tools::ToolExecutionConsumer;
use piko_protocol::PersistEvent;

use super::AgentRunDeps;

pub(super) struct TaskContext {
    identity: DispatchIdentity,
    parent_task_id: Option<String>,
    prompt: String,
    source_agent_id: Option<String>,
    resumed: bool,
}

impl TaskContext {
    pub(super) fn is_resumed(&self) -> bool {
        self.resumed
    }
    pub(super) fn new(task: &AgentTask, spec: &AgentSpec) -> Self {
        let task_id = task.id.clone().unwrap_or_default();
        let agent_id = spec.id.clone();
        let host_context = task.host_context.clone();
        let session_id = host_context
            .as_ref()
            .map(|hc| hc.session_id.clone())
            .unwrap_or_else(|| task_id.clone());
        let identity = DispatchIdentity::new(session_id, task_id, agent_id);
        let parent_task_id = task.parent_task_id.clone();
        let prompt = task.prompt.clone();
        let source_agent_id = match &task.source {
            piko_protocol::agents::TaskSource::Agent { agent_id, .. } => Some(agent_id.clone()),
            _ => None,
        };

        Self {
            identity,
            parent_task_id,
            prompt,
            source_agent_id,
            resumed: task.resume.is_some(),
        }
    }

    pub(super) fn agent_id(&self) -> &str {
        self.identity.agent_id()
    }

    pub(super) fn session_id(&self) -> &str {
        self.identity.session_id()
    }

    pub(super) fn task_id(&self) -> &str {
        self.identity.task_id()
    }

    pub(super) fn source_agent_id(&self) -> Option<&str> {
        self.source_agent_id.as_deref()
    }

    pub(super) fn parent_task_id(&self) -> Option<&str> {
        self.parent_task_id.as_deref()
    }

    pub(super) fn prompt(&self) -> &str {
        &self.prompt
    }

    pub(super) async fn commit_user_input(
        &self,
        input: &piko_protocol::agent_runtime::SubmitTaskInput,
        emitter: TaskEventEmitter,
    ) {
        let event = PersistEvent::UserCommitted {
            session_id: self.identity.session_id().clone(),
            message_id: input.message_id.clone(),
            task_id: self.identity.task_id().clone(),
            agent_id: self.identity.agent_id().clone(),
            work_id: input.work_id.clone(),
            message: piko_protocol::Message::User {
                content: input.content.clone(),
                timestamp: Some(input.submitted_at),
            },
        };
        emitter.emit_persist(event).await;
    }

    pub(super) fn lifecycle_consumer(&self, emitter: TaskEventEmitter) -> TaskLifecycleConsumer {
        TaskLifecycleConsumer::new(emitter)
    }

    pub(super) fn tool_discovery_context(&self, spec: &AgentSpec) -> ToolDiscoveryContext {
        ToolDiscoveryContext {
            agent_id: self.identity.agent_id().clone(),
            task_id: Some(self.identity.task_id().clone()),
            tool_set_ids: spec.tool_set_ids.clone(),
            active_tool_names: spec.active_tool_names.clone(),
        }
    }

    pub(super) fn assistant_message_id(&self, step_count: u32) -> String {
        runtime_assistant_message_id(self.task_id(), &format!("step_{step_count}"))
    }

    pub(super) fn dispatch_identity(&self) -> DispatchIdentity {
        self.identity.clone()
    }

    pub(super) fn tool_execution_consumer(
        &self,
        emitter: TaskEventEmitter,
        message_id: String,
        work_id: String,
        source_turn_id: Option<String>,
    ) -> ToolExecutionConsumer {
        ToolExecutionConsumer::with_emitter(
            emitter,
            self.identity.clone(),
            work_id,
            source_turn_id,
            message_id,
        )
    }

    pub(super) fn gateway_request(
        &self,
        deps: &AgentRunDeps,
        spec: &AgentSpec,
        transcript: &TranscriptManager,
        model: &ModelSpec,
        step_id: String,
        tools: Vec<piko_protocol::tools::ToolDef>,
    ) -> GatewayRequest {
        GatewayRequest {
            run_id: self.identity.task_id().clone(),
            step_id,
            transcript: transcript.to_vec(),
            system_prompt: spec.system_prompt.clone(),
            model: model.id.clone(),
            provider: model.provider.clone(),
            tools,
            thinking: deps
                .model_config
                .as_ref()
                .and_then(|c| c.resolve_thinking()),
        }
    }

    pub(super) fn step_dispatch(
        &self,
        message_id: String,
        work_id: String,
        model: ModelSpec,
        llm: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> StepDispatch {
        StepDispatch::from_step_stream(self.identity.clone(), message_id, work_id, model, llm)
    }

    pub(super) fn step_failure_dispatch(
        &self,
        message_id: String,
        work_id: String,
        model: ModelSpec,
        error_message: String,
    ) -> StepDispatch {
        StepDispatch::from_step_failure(
            self.identity.clone(),
            message_id,
            work_id,
            model,
            error_message,
        )
    }
}
