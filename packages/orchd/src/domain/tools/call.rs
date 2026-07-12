// ---- Domain: tool call (re-exports from piko_protocol) ----

pub use piko_protocol::messages::ToolCall;

/// A tool call entity produced by step dispatch and consumed by the tool executor.
#[derive(Clone, Debug)]
pub struct ToolCallItem {
    pub tool_call_index: u32,
    pub content_index: u32,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
