use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::stream::{AgentRunDeps, TranscriptManager};
use crate::runtime::tool_calls::ToolCallItem;
use crate::runtime::tool_executor::{self, ToolExecutionResult};
use piko_protocol::{DisplayEvent, Message, PersistEvent};

#[derive(Clone)]
pub struct ToolExecutionConsumer {
    dispatcher: ToolExecutionDispatcher,
}

impl ToolExecutionConsumer {
    pub(crate) fn new(
        senders: Option<DispatchSenders>,
        host_context: Option<HostTaskContext>,
        task_id: String,
        agent_id: String,
        parent_message_id: String,
    ) -> Self {
        Self {
            dispatcher: ToolExecutionDispatcher::new(
                senders,
                host_context,
                task_id,
                agent_id,
                parent_message_id,
            ),
        }
    }

    pub(crate) async fn execute_tool_calls(
        &self,
        deps: &AgentRunDeps,
        spawner: &Arc<dyn AgentSpawner>,
        tool_calls: &[ToolCallItem],
        routes: &std::collections::HashMap<String, CatalogRoute>,
        model_settings: &ModelRunSettings,
        cancel: CancellationToken,
        transcript: &mut TranscriptManager,
        turn_index: u32,
    ) -> Result<ToolExecutionResult, String> {
        let result = tool_executor::execute_tool_calls_with_deps(
            deps,
            spawner,
            tool_calls,
            routes,
            model_settings,
            cancel,
            transcript,
            turn_index,
            self,
        )
        .await?;
        tracing::debug!(
            task_id = %self.dispatcher.task_id,
            agent_id = %self.dispatcher.agent_id,
            completed_calls = result.completed_calls,
            failed_calls = result.failed_calls,
            "tool execution finished"
        );
        Ok(result)
    }

    pub(crate) fn agent_id(&self) -> &str {
        &self.dispatcher.agent_id
    }

    pub(crate) fn task_id(&self) -> &str {
        &self.dispatcher.task_id
    }

    pub(crate) fn parent_message_id(&self) -> &str {
        &self.dispatcher.parent_message_id
    }

    pub(crate) fn host_context(&self) -> Option<&HostTaskContext> {
        self.dispatcher.host_context.as_ref()
    }

    pub(crate) fn senders(&self) -> &Option<DispatchSenders> {
        &self.dispatcher.senders
    }

    pub(crate) async fn tool_started(
        &self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    ) -> Option<Event> {
        self.dispatcher
            .tool_started(tool_call_id, tool_name, args)
            .await
    }

    pub(crate) async fn tool_ended(
        &self,
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    ) -> Option<Event> {
        self.dispatcher
            .tool_ended(tool_call_id, tool_name, result, is_error)
            .await
    }

    pub(crate) async fn tool_result_committed(
        &self,
        tool_call_index: u32,
        message: Message,
    ) -> Option<Event> {
        self.dispatcher
            .tool_result_committed(tool_call_index, message)
            .await
    }
}

#[derive(Clone)]
pub struct ToolExecutionDispatcher {
    pub(crate) senders: Option<DispatchSenders>,
    pub(crate) host_context: Option<HostTaskContext>,
    pub(crate) task_id: String,
    pub(crate) agent_id: String,
    pub(crate) parent_message_id: String,
}

impl ToolExecutionDispatcher {
    pub(crate) fn new(
        senders: Option<DispatchSenders>,
        host_context: Option<HostTaskContext>,
        task_id: String,
        agent_id: String,
        parent_message_id: String,
    ) -> Self {
        Self {
            senders,
            host_context,
            task_id,
            agent_id,
            parent_message_id,
        }
    }

    pub(crate) async fn tool_started(
        &self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    ) -> Option<Event> {
        let event = DisplayEvent::ToolStarted {
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            tool_call_id,
            tool_name,
            args,
            parent_message_id: Some(self.parent_message_id.clone()),
        };
        if let Some(ref s) = self.senders {
            let _ = s.display.send(Arc::new(event)).await;
            None
        } else {
            Some(Event::Display(event))
        }
    }

    pub(crate) async fn tool_ended(
        &self,
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    ) -> Option<Event> {
        let event = DisplayEvent::ToolEnded {
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            tool_call_id,
            tool_name,
            result,
            is_error,
        };
        if let Some(ref s) = self.senders {
            let _ = s.display.send(Arc::new(event)).await;
            None
        } else {
            Some(Event::Display(event))
        }
    }

    pub(crate) async fn tool_result_committed(
        &self,
        tool_call_index: u32,
        message: Message,
    ) -> Option<Event> {
        let hc = self.host_context.as_ref()?;
        let message_id = format!("{}:tool_result:{}", self.parent_message_id, tool_call_index);
        let event = PersistEvent::ToolResultCommitted {
            session_id: hc.session_id.clone(),
            message_id,
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            message,
        };
        if let Some(ref s) = self.senders {
            let _ = s.persist.send(Arc::new(event)).await;
            None
        } else {
            Some(Event::Persist(event))
        }
    }
}
