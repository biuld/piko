use std::pin::Pin;

use futures_core::Stream;
use llmd::gateway::{GatewayEvent, GatewayRequest};

use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelSpec;
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::ports::tool_provider::ToolDiscoveryContext;
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::dispatch::StepDispatch;
use crate::runtime::dispatch::ToolExecutionConsumer;
use crate::runtime::dispatch::consumer::{
    AgentDispatchContext, DispatchIdentity, lifecycle::TaskLifecycleConsumer,
};
use crate::runtime::runtime_assistant_message_id;

use super::AgentRunDeps;

pub(super) struct TaskContext {
    task_id: String,
    agent_id: String,
    host_context: Option<HostTaskContext>,
    identity: DispatchIdentity,
    turn_id: String,
    parent_task_id: Option<String>,
    prompt: String,
    source_agent_id: Option<String>,
}

impl TaskContext {
    pub(super) fn new(task: &AgentTask, spec: &AgentSpec) -> Self {
        let task_id = task.id.clone().unwrap_or_default();
        let agent_id = spec.id.clone();
        let host_context = task.host_context.clone();
        let session_id = host_context
            .as_ref()
            .map(|hc| hc.session_id.clone())
            .unwrap_or_else(|| task_id.clone());
        let identity = DispatchIdentity::new(session_id, task_id.clone(), agent_id.clone());
        let turn_id = host_context
            .as_ref()
            .map(|hc| hc.turn_id.clone())
            .unwrap_or_else(|| task_id.clone());
        let parent_task_id = task.parent_task_id.clone();
        let prompt = task.prompt.clone();
        let source_agent_id = match &task.source {
            piko_protocol::agents::TaskSource::Agent { agent_id, .. } => Some(agent_id.clone()),
            _ => None,
        };

        Self {
            task_id,
            agent_id,
            host_context,
            identity,
            turn_id,
            parent_task_id,
            prompt,
            source_agent_id,
        }
    }

    pub(super) fn task_id(&self) -> &str {
        &self.task_id
    }

    pub(super) fn task_id_owned(&self) -> String {
        self.task_id.clone()
    }

    pub(super) fn agent_id_owned(&self) -> String {
        self.agent_id.clone()
    }

    pub(super) fn host_context_owned(&self) -> Option<HostTaskContext> {
        self.host_context.clone()
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

    pub(super) fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub(super) fn session_id(&self) -> String {
        self.identity.session_id().clone()
    }

    pub(super) fn dispatch_identity(&self) -> DispatchIdentity {
        self.identity.clone()
    }

    pub(super) fn dispatch_context<'a>(
        &'a self,
        session_id: &'a String,
        message_id: &'a String,
    ) -> AgentDispatchContext<'a> {
        AgentDispatchContext {
            session_id,
            task_id: self.identity.task_id(),
            agent_id: self.identity.agent_id(),
            message_id,
            model: None,
        }
    }

    pub(super) fn lifecycle_consumer(
        &self,
        senders: Option<DispatchSenders>,
    ) -> TaskLifecycleConsumer {
        TaskLifecycleConsumer::new(
            senders,
            self.dispatch_identity(),
            self.turn_id().to_string(),
        )
    }

    pub(super) fn tool_discovery_context(&self, spec: &AgentSpec) -> ToolDiscoveryContext {
        ToolDiscoveryContext {
            agent_id: self.agent_id_owned(),
            task_id: Some(self.task_id_owned()),
            tool_set_ids: spec.tool_set_ids.clone(),
            active_tool_names: spec.active_tool_names.clone(),
        }
    }

    pub(super) fn assistant_message_id(&self, step_count: u32) -> String {
        runtime_assistant_message_id(self.task_id(), &format!("step_{}", step_count))
    }

    pub(super) fn tool_execution_consumer(
        &self,
        senders: Option<DispatchSenders>,
        message_id: String,
    ) -> ToolExecutionConsumer {
        ToolExecutionConsumer::new(
            senders,
            self.host_context_owned(),
            self.dispatch_identity(),
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
            run_id: self.task_id_owned(),
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
        model: ModelSpec,
        llm: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> StepDispatch {
        let (session_id, task_id, agent_id) = self.dispatch_identity().into_parts();
        StepDispatch::from_step_stream(session_id, task_id, agent_id, message_id, model, llm)
    }

    pub(super) fn step_failure_dispatch(
        &self,
        message_id: String,
        model: ModelSpec,
        error_message: String,
    ) -> StepDispatch {
        let (session_id, task_id, agent_id) = self.dispatch_identity().into_parts();
        StepDispatch::from_step_failure(
            session_id,
            task_id,
            agent_id,
            message_id,
            model,
            error_message,
        )
    }
}
