//! Tool call aggregation for Model Step dispatch (Execution collecting path).

use std::sync::Arc;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::domain::RealtimeFrame;
use crate::domain::tools::call::ToolCallItem;
use crate::runtime::events::collector::{SharedPersistCollector, SharedRealtimeCollector};
use crate::runtime::events::identity::{AgentDispatchContext, DispatchIdentity, StepEventConsumer};
use crate::runtime::utils::now_ms;
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{Message, PersistEvent};

#[derive(Clone)]
struct InFlightToolCall {
    tool_call_index: u32,
    id: String,
    name: String,
    arguments_json: String,
}

#[allow(dead_code)]
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

/// Shared slot that `ToolCallDispatchConsumer` fills in `on_step_finished`.
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

pub struct ToolCallDispatchConsumer {
    identity: DispatchIdentity,
    aggregator: ToolCallAggregator,
    pending_commits: Vec<PendingToolCallCommit>,
    tool_call_collector: SharedToolCallCollector,
    realtime_collector: SharedRealtimeCollector,
    persist_collector: SharedPersistCollector,
}

struct PendingToolCallCommit {
    message_id: String,
    message: Message,
}

impl ToolCallDispatchConsumer {
    pub(crate) fn for_collecting(
        identity: DispatchIdentity,
        tool_call_collector: SharedToolCallCollector,
        realtime_collector: SharedRealtimeCollector,
        persist_collector: SharedPersistCollector,
    ) -> Self {
        Self {
            identity,
            aggregator: ToolCallAggregator::new(),
            pending_commits: Vec::new(),
            tool_call_collector,
            realtime_collector,
            persist_collector,
        }
    }
}

#[async_trait]
impl StepEventConsumer for ToolCallDispatchConsumer {
    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        let Some(update) = self.aggregator.on_gateway_event(event) else {
            return;
        };
        self.realtime_collector.push(RealtimeFrame::new(
            ctx.task_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::ToolCall {
                content_index: update.content_index,
                tool_call_id: update.tool_call_id,
                delta: update.delta,
            },
        ));
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
            self.tool_call_collector.push(tool_call);
            self.pending_commits.push(PendingToolCallCommit {
                message_id,
                message,
            });
        }
    }

    async fn on_assistant_message_committed(
        &mut self,
        ctx: &AgentDispatchContext<'_>,
        _message: &Message,
        _tool_calls: &[ToolCallItem],
    ) {
        for commit in std::mem::take(&mut self.pending_commits) {
            self.persist_collector.push(PersistEvent::ToolCallCommitted {
                session_id: self.identity.session_id().clone(),
                message_id: commit.message_id,
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                work_id: ctx.work_id.to_string(),
                parent_message_id: ctx.message_id.clone(),
                message: commit.message,
            });
        }
    }
}
