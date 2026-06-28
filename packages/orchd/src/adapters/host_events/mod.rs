// ---- Adapters: host events — bridge to the host/TUI layer ----
//
// Centralized host event emission utilities.
// Previously scattered across ActorActor::finalize(), OrchCore::init(), and
// task spawning. This module consolidates event producing helpers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::events::event::Event;
use crate::domain::events::sink::SharedEventSink;
use crate::domain::model::transcript::ContentBlock;
use crate::domain::model::usage::HostUsage;
use piko_protocol::{ToolCallRef, UsageCost};

/// Emit an event through the shared event sink.
pub async fn emit_host(sink: &SharedEventSink, event: Event) {
    (sink)(event).await;
}

/// Current timestamp in milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Build a SharedEventSink from a hashmap of listeners.
pub fn sink_from_listeners(
    listeners: Arc<
        tokio::sync::RwLock<
            HashMap<u64, Arc<dyn Fn(serde_json::Value) + Send + Sync>>,
        >,
    >,
) -> SharedEventSink {
    Arc::new(move |event: Event| {
        let listeners = Arc::clone(&listeners);
        Box::pin(async move {
            let val = serde_json::to_value(&event).unwrap_or_default();
            let ls = listeners.read().await;
            for listener in ls.values() {
                listener(val.clone());
            }
        })
    })
}

/// Extract text from ContentBlock slices.
pub fn text_from_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Extract tool calls from ContentBlock slices.
pub fn tool_calls_from_blocks(blocks: &[ContentBlock]) -> Vec<ToolCallRef> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Some(ToolCallRef {
                id: id.clone(),
                name: name.clone(),
                args: arguments.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Convert MessageUsage to HostUsage.
pub fn host_usage_from_message_usage(usage: &crate::domain::model::transcript::MessageUsage) -> HostUsage {
    use piko_protocol::Usage as HostUsage;
    HostUsage {
        input: usage.input,
        output: usage.output,
        cache_read: usage.cache_read,
        cache_write: usage.cache_write,
        total_tokens: usage.total_tokens,
        cost: UsageCost {
            input: usage.cost.input,
            output: usage.cost.output,
            cache_read: usage.cost.cache_read,
            cache_write: usage.cost.cache_write,
            total: usage.cost.total,
        },
    }
}
