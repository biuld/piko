// ---- Tool runtime consumer — step dispatch (aggregation) + tool execution ----
//
// `ToolExecutionConsumer` is the single tool runtime consumer.  It has two roles:
//
// **Step dispatch role** (registered via `StepDispatch::register_consumer`):
//   Built with `for_step_dispatch_channel` / `for_step_dispatch_collecting`.
//   - `on_gateway_event`: feeds `ToolCallAggregator`, emits `DisplayEvent::ToolCallDelta`.
//   - `on_step_finished`: flushes aggregator, emits `PersistEvent::ToolCallCommitted`,
//     and fills `SharedToolCallCollector` so the orchestrator can retrieve complete tool calls.
//
// **Execution role** (built with `new`, driven by `execute_tool_calls`):
//   - Wraps `tool_executor::execute_tool_calls_with_deps`.
//   - Emits `ToolStarted`, `ToolEnded`, `ToolResultCommitted` for each call.

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

use super::{AgentDispatchContext, AgentEventConsumer};
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

/// Shared slot that `ToolExecutionConsumer` (step role) fills in `on_step_finished`.
/// The step dispatch result reads from it to populate `StepDispatchResult.tool_calls`.
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

// ─── ToolExecutionConsumer ────────────────────────────────────────────────────

pub struct ToolExecutionConsumer {
    // Shared context (both roles)
    senders: Option<DispatchSenders>,
    host_context: Option<HostTaskContext>,
    task_id: String,
    agent_id: String,
    parent_message_id: String,
    session_id: String,
    // Step dispatch role only
    aggregator: ToolCallAggregator,
    tool_call_collector: Option<SharedToolCallCollector>,
    display_collector: Option<SharedDisplayCollector>,
    persist_collector: Option<SharedPersistCollector>,
}

impl Clone for ToolExecutionConsumer {
    /// Clones produce a fresh execution-role consumer with an empty aggregator and no collectors.
    /// Used by parallel tool execution so each future gets its own independent emit handle.
    fn clone(&self) -> Self {
        Self {
            senders: self.senders.clone(),
            host_context: self.host_context.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            parent_message_id: self.parent_message_id.clone(),
            session_id: self.session_id.clone(),
            aggregator: ToolCallAggregator::new(),
            tool_call_collector: None,
            display_collector: None,
            persist_collector: None,
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
        task_id: String,
        agent_id: String,
        parent_message_id: String,
    ) -> Self {
        let session_id = host_context
            .as_ref()
            .map(|hc| hc.session_id.clone())
            .unwrap_or_default();
        Self {
            senders,
            host_context,
            task_id,
            agent_id,
            parent_message_id,
            session_id,
            aggregator: ToolCallAggregator::new(),
            tool_call_collector: None,
            display_collector: None,
            persist_collector: None,
        }
    }

    /// Step-dispatch consumer — **channel mode** (`senders` is `Some`).
    /// Aggregates tool call chunks and sends events to the typed channels.
    pub(crate) fn for_step_dispatch_channel(
        senders: DispatchSenders,
        session_id: String,
        task_id: String,
        agent_id: String,
        parent_message_id: String,
        tool_call_collector: SharedToolCallCollector,
    ) -> Self {
        Self {
            senders: Some(senders),
            host_context: None,
            task_id,
            agent_id,
            parent_message_id,
            session_id,
            aggregator: ToolCallAggregator::new(),
            tool_call_collector: Some(tool_call_collector),
            display_collector: None,
            persist_collector: None,
        }
    }

    /// Step-dispatch consumer — **collecting mode** (`senders` is `None`).
    /// Aggregates tool call chunks and pushes events into local collectors.
    pub(crate) fn for_step_dispatch_collecting(
        session_id: String,
        task_id: String,
        agent_id: String,
        parent_message_id: String,
        tool_call_collector: SharedToolCallCollector,
        display_collector: SharedDisplayCollector,
        persist_collector: SharedPersistCollector,
    ) -> Self {
        Self {
            senders: None,
            host_context: None,
            task_id,
            agent_id,
            parent_message_id,
            session_id,
            aggregator: ToolCallAggregator::new(),
            tool_call_collector: Some(tool_call_collector),
            display_collector: Some(display_collector),
            persist_collector: Some(persist_collector),
        }
    }

    // ─── Accessors (used by tool_executor) ───────────────────────────────────

    pub(crate) fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub(crate) fn task_id(&self) -> &str {
        &self.task_id
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
            task_id = %self.task_id,
            agent_id = %self.agent_id,
            completed_calls = result.completed_calls,
            failed_calls = result.failed_calls,
            "tool execution finished"
        );
        Ok(result)
    }

    // ─── Tool lifecycle emit (called by tool_executor) ────────────────────────

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

// ─── AgentEventConsumer impl (step dispatch role) ────────────────────────────
//
// When registered inside `StepDispatch`, this consumer handles the full tool call lifecycle
// for a single model step:
//   gateway event → aggregate chunks → emit ToolCallDelta
//   step finished → flush aggregator → emit ToolCallCommitted → fill SharedToolCallCollector

#[async_trait]
impl AgentEventConsumer for ToolExecutionConsumer {
    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        let Some(update) = self.aggregator.on_gateway_event(event) else {
            return;
        };
        let display_event = DisplayEvent::ToolCallDelta {
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            message_id: ctx.message_id.clone(),
            content_index: update.content_index,
            tool_call_id: update.tool_call_id,
            delta: update.delta,
        };
        if let Some(ref s) = self.senders {
            let _ = s.display.send(Arc::new(display_event)).await;
        } else if let Some(ref dc) = self.display_collector {
            dc.push(display_event);
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let tool_calls = self.aggregator.flush();
        if tool_calls.is_empty() {
            return;
        }
        let Some(ref tc_collector) = self.tool_call_collector else {
            return;
        };
        for tool_call in tool_calls {
            let message_id = format!("{}:tool_call:{}", ctx.message_id, tool_call.tool_call_index);
            let message = Message::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                model: Some(ctx.model.id.clone()),
                provider: Some(ctx.model.provider.clone()),
                timestamp: Some(now_ms()),
            };
            let persist_event = PersistEvent::ToolCallCommitted {
                session_id: self.session_id.clone(),
                message_id,
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                parent_message_id: ctx.message_id.clone(),
                message,
            };
            if let Some(ref s) = self.senders {
                let _ = s.persist.send(Arc::new(persist_event)).await;
            } else if let Some(ref pc) = self.persist_collector {
                pc.push(persist_event);
            }
            tc_collector.push(tool_call);
        }
    }
}
