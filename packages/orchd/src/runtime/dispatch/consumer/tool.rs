// ---- Tool runtime consumers ----
//
// `ToolCallDispatchConsumer` handles step-dispatch aggregation:
//   gateway tool call chunks -> tool call deltas -> committed tool calls.
//
// `ToolExecutionConsumer` handles execution-time emission:
//   tool started / ended / result committed.

use std::sync::Arc;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::orchestrator::AgentRunDeps;
use crate::runtime::tool_executor::{self, ToolExecutionResult};
use crate::runtime::types::ToolCallItem;
use crate::runtime::utils::now_ms;
use piko_protocol::{DisplayEvent, Message, PersistEvent};

use super::{AgentDispatchContext, AgentEventConsumer, DispatchIdentity};
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::dispatch::step::collectors::{SharedDisplayCollector, SharedPersistCollector};

// ─── ToolCallAggregator ──────────────────────────────────────────────────────

#[derive(Clone)]
struct InFlightToolCall {
    tool_call_index: u32,
    id: String,
    name: String,
    arguments_json: String,
}

pub struct ToolCallChunkUpdate {
    pub content_index: u32,
    pub tool_call_index: u32,
    pub tool_call_id: String,
    pub delta: String,
}

#[derive(Default, Clone)]
pub struct ToolCallAggregator {
    next_tool_call_index: u32,
    current: Option<InFlightToolCall>,
    completed: Vec<ToolCallItem>,
}

impl ToolCallAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_gateway_event(&mut self, event: &GatewayEvent) -> Option<ToolCallChunkUpdate> {
        match event {
            GatewayEvent::ToolCallChunk {
                id,
                name,
                args_delta,
            } => self.on_chunk(id.clone(), name.clone(), args_delta.clone()),
            _ => None,
        }
    }

    pub fn on_chunk(
        &mut self,
        id: String,
        name: String,
        args_delta: String,
    ) -> Option<ToolCallChunkUpdate> {
        if !name.is_empty() {
            self.finalize_current();
            let tool_call_index = self.next_tool_call_index;
            self.next_tool_call_index += 1;
            self.current = Some(InFlightToolCall {
                tool_call_index,
                id: id.clone(),
                name,
                arguments_json: args_delta.clone(),
            });
            return Some(ToolCallChunkUpdate {
                content_index: tool_call_index,
                tool_call_index,
                tool_call_id: id,
                delta: args_delta,
            });
        }

        let current = self.current.as_mut()?;
        current.arguments_json.push_str(&args_delta);
        Some(ToolCallChunkUpdate {
            content_index: current.tool_call_index,
            tool_call_index: current.tool_call_index,
            tool_call_id: current.id.clone(),
            delta: args_delta,
        })
    }

    pub fn flush(&mut self) -> Vec<ToolCallItem> {
        self.finalize_current();
        std::mem::take(&mut self.completed)
    }

    fn finalize_current(&mut self) {
        let Some(current) = self.current.take() else {
            return;
        };

        let arguments = match serde_json::from_str::<serde_json::Value>(&current.arguments_json) {
            Ok(arguments) => arguments,
            Err(_) => serde_json::Value::String(current.arguments_json),
        };

        self.completed.push(ToolCallItem {
            content_index: current.tool_call_index,
            tool_call_index: current.tool_call_index,
            id: current.id,
            name: current.name,
            arguments,
        });
    }
}

// ─── SharedToolCallCollector ─────────────────────────────────────────────────

/// Shared slot that `ToolCallDispatchConsumer` fills in `on_step_finished`.
/// The step dispatch result reads from it to populate `CompletedStep.tool_calls`.
#[derive(Clone, Default)]
pub(crate) struct SharedToolCallCollector(Arc<std::sync::Mutex<Vec<ToolCallItem>>>);

impl SharedToolCallCollector {
    pub(crate) fn take(&self) -> Vec<ToolCallItem> {
        let mut tc = self.0.lock().expect("tool call collector poisoned");
        std::mem::take(&mut *tc)
    }

    pub(crate) fn push(&self, item: ToolCallItem) {
        self.0
            .lock()
            .expect("tool call collector poisoned")
            .push(item);
    }
}

#[derive(Clone, Default)]
struct SharedExecutionEventCollector(Arc<std::sync::Mutex<Vec<Event>>>);

impl SharedExecutionEventCollector {
    fn take(&self) -> Vec<Event> {
        let mut events = self.0.lock().expect("execution event collector poisoned");
        std::mem::take(&mut *events)
    }

    fn push(&self, event: Event) {
        self.0
            .lock()
            .expect("execution event collector poisoned")
            .push(event);
    }
}

// ─── ToolCallDispatchConsumer ────────────────────────────────────────────────

pub struct ToolCallDispatchConsumer {
    senders: Option<DispatchSenders>,
    session_id: String,
    aggregator: ToolCallAggregator,
    tool_call_collector: SharedToolCallCollector,
    display_collector: Option<SharedDisplayCollector>,
    persist_collector: Option<SharedPersistCollector>,
}

impl ToolCallDispatchConsumer {
    pub(crate) fn for_channel(
        senders: DispatchSenders,
        session_id: String,
        tool_call_collector: SharedToolCallCollector,
    ) -> Self {
        Self {
            senders: Some(senders),
            session_id,
            aggregator: ToolCallAggregator::new(),
            tool_call_collector,
            display_collector: None,
            persist_collector: None,
        }
    }

    pub(crate) fn for_collecting(
        session_id: String,
        tool_call_collector: SharedToolCallCollector,
        display_collector: SharedDisplayCollector,
        persist_collector: SharedPersistCollector,
    ) -> Self {
        Self {
            senders: None,
            session_id,
            aggregator: ToolCallAggregator::new(),
            tool_call_collector,
            display_collector: Some(display_collector),
            persist_collector: Some(persist_collector),
        }
    }

    async fn emit_display_event(&self, event: DisplayEvent) {
        if let Some(ref s) = self.senders {
            let _ = s.display.send(Arc::new(event)).await;
        } else if let Some(ref dc) = self.display_collector {
            dc.push(event);
        }
    }

    async fn emit_persist_event(&self, event: PersistEvent) {
        if let Some(ref s) = self.senders {
            let _ = s.persist.send(Arc::new(event)).await;
        } else if let Some(ref pc) = self.persist_collector {
            pc.push(event);
        }
    }
}

#[async_trait]
impl AgentEventConsumer for ToolCallDispatchConsumer {
    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        let Some(update) = self.aggregator.on_gateway_event(event) else {
            return;
        };
        self.emit_display_event(DisplayEvent::ToolCallDelta {
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            message_id: ctx.message_id.clone(),
            content_index: update.content_index,
            tool_call_id: update.tool_call_id,
            delta: update.delta,
        })
        .await;
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        for tool_call in self.aggregator.flush() {
            let message_id = format!("{}:tool_call:{}", ctx.message_id, tool_call.tool_call_index);
            let message = Message::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                model: Some(ctx.model.expect("step dispatch model missing").id.clone()),
                provider: Some(
                    ctx.model
                        .expect("step dispatch model missing")
                        .provider
                        .clone(),
                ),
                timestamp: Some(now_ms()),
            };
            self.emit_persist_event(PersistEvent::ToolCallCommitted {
                session_id: self.session_id.clone(),
                message_id,
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                parent_message_id: ctx.message_id.clone(),
                message,
            })
            .await;
            self.tool_call_collector.push(tool_call);
        }
    }
}

// ─── ToolExecutionConsumer ────────────────────────────────────────────────────

pub struct ToolExecutionConsumer {
    senders: Option<DispatchSenders>,
    host_context: Option<HostTaskContext>,
    identity: DispatchIdentity,
    parent_message_id: String,
    execution_event_collector: Option<SharedExecutionEventCollector>,
}

impl Clone for ToolExecutionConsumer {
    /// Clones produce a fresh execution-role consumer with an empty aggregator and no collectors.
    /// Used by parallel tool execution so each future gets its own independent emit handle.
    fn clone(&self) -> Self {
        Self {
            senders: self.senders.clone(),
            host_context: self.host_context.clone(),
            identity: self.identity.clone(),
            parent_message_id: self.parent_message_id.clone(),
            execution_event_collector: self.execution_event_collector.clone(),
        }
    }
}

impl ToolExecutionConsumer {
    // ─── Constructors ────────────────────────────────────────────────────────

    /// Execution-only consumer.  Does not aggregate tool call chunks.
    /// Use this constructor in `TaskOrchestrator::execute_tool_calls`.
    pub(crate) fn new(
        senders: Option<DispatchSenders>,
        host_context: Option<HostTaskContext>,
        identity: DispatchIdentity,
        parent_message_id: String,
    ) -> Self {
        Self {
            senders,
            host_context,
            identity,
            parent_message_id,
            execution_event_collector: Some(SharedExecutionEventCollector::default()),
        }
    }

    // ─── Accessors (used by tool_executor) ───────────────────────────────────

    pub(crate) fn agent_id(&self) -> &str {
        self.identity.agent_id()
    }

    pub(crate) fn task_id(&self) -> &str {
        self.identity.task_id()
    }

    pub(crate) fn parent_message_id(&self) -> &str {
        &self.parent_message_id
    }

    pub(crate) fn host_context(&self) -> Option<&HostTaskContext> {
        self.host_context.as_ref()
    }

    pub(crate) fn senders(&self) -> &Option<DispatchSenders> {
        &self.senders
    }

    pub(crate) fn tool_result_message_id(&self, tool_call_index: u32) -> String {
        format!("{}:tool_result:{}", self.parent_message_id, tool_call_index)
    }

    // ─── Execution entry point ───────────────────────────────────────────────

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
        let mut result = tool_executor::execute_tool_calls_with_deps(
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
            task_id = %self.identity.task_id(),
            agent_id = %self.identity.agent_id(),
            completed_calls = result.completed_calls,
            failed_calls = result.failed_calls,
            "tool execution finished"
        );
        if let Some(ref collector) = self.execution_event_collector {
            let mut emitted = collector.take();
            emitted.append(&mut result.events);
            result.events = emitted;
        }
        Ok(result)
    }

    // ─── Tool lifecycle emit (called by tool_executor) ────────────────────────

    async fn emit_display_event(&self, event: DisplayEvent) -> Option<Event> {
        if let Some(ref s) = self.senders {
            let _ = s.display.send(Arc::new(event)).await;
            None
        } else if let Some(ref collector) = self.execution_event_collector {
            let runtime_event = Event::Display(event);
            collector.push(runtime_event.clone());
            Some(runtime_event)
        } else {
            Some(Event::Display(event))
        }
    }

    async fn emit_persist_event(&self, event: PersistEvent) -> Option<Event> {
        if let Some(ref s) = self.senders {
            let _ = s.persist.send(Arc::new(event)).await;
            None
        } else if let Some(ref collector) = self.execution_event_collector {
            let runtime_event = Event::Persist(event);
            collector.push(runtime_event.clone());
            Some(runtime_event)
        } else {
            Some(Event::Persist(event))
        }
    }

    pub(crate) async fn emit_tool_started(&self, tool_call: &ToolCallItem) {
        let _ = self
            .emit_display_event(DisplayEvent::ToolStarted {
                task_id: self.identity.task_id().clone(),
                agent_id: self.identity.agent_id().clone(),
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                args: tool_call.arguments.clone(),
                parent_message_id: Some(self.parent_message_id.clone()),
            })
            .await;
    }

    pub(crate) async fn emit_tool_ended(
        &self,
        tool_call: &ToolCallItem,
        result: &serde_json::Value,
        is_error: bool,
    ) {
        let _ = self
            .emit_display_event(DisplayEvent::ToolEnded {
                task_id: self.identity.task_id().clone(),
                agent_id: self.identity.agent_id().clone(),
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                result: result.clone(),
                is_error,
            })
            .await;
    }

    pub(crate) async fn emit_tool_result_committed(&self, message: &Message, msg_id: &str) {
        let _ = self
            .emit_persist_event(PersistEvent::ToolResultCommitted {
                session_id: self.identity.session_id().clone(),
                message_id: msg_id.to_string(),
                task_id: self.identity.task_id().clone(),
                agent_id: self.identity.agent_id().clone(),
                message: message.clone(),
            })
            .await;
    }
}
