use serde::{Deserialize, Serialize};

use crate::messages::{ContentBlock, Message};

pub type EntryId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionTreeEntry {
    #[serde(rename = "message")]
    Message(MessageEntry),
    #[serde(rename = "tool_call")]
    ToolCall(ToolCallEntry),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    #[serde(rename = "model_change")]
    ModelChange(ModelChangeEntry),
    #[serde(rename = "active_tools_change")]
    ActiveToolsChange(ActiveToolsChangeEntry),
    #[serde(rename = "compaction")]
    Compaction(CompactionEntry),
    #[serde(rename = "branch_summary")]
    BranchSummary(BranchSummaryEntry),
    #[serde(rename = "custom")]
    Custom(CustomEntry),
    #[serde(rename = "custom_message")]
    CustomMessage(CustomMessageEntry),
    #[serde(rename = "label")]
    Label(LabelEntry),
    #[serde(rename = "session_info")]
    SessionInfo(SessionInfoEntry),
    #[serde(rename = "leaf")]
    Leaf(LeafEntry),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub agent_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_seq: u64,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelChangeEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub thinking_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelChangeEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveToolsChangeEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub active_tool_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub summary: String,
    pub first_kept_entry_id: String,
    #[serde(default, alias = "totalTokens")]
    pub tokens_before: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hook: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub from_id: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hook: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub custom_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CustomMessageContent {
    String(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomMessageEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub custom_type: String,
    pub content: CustomMessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub display: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LeafEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub target_id: Option<EntryId>,
}

impl SessionTreeEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::Message(entry) => &entry.id,
            Self::ToolCall(entry) => &entry.id,
            Self::ThinkingLevelChange(entry) => &entry.id,
            Self::ModelChange(entry) => &entry.id,
            Self::ActiveToolsChange(entry) => &entry.id,
            Self::Compaction(entry) => &entry.id,
            Self::BranchSummary(entry) => &entry.id,
            Self::Custom(entry) => &entry.id,
            Self::CustomMessage(entry) => &entry.id,
            Self::Label(entry) => &entry.id,
            Self::SessionInfo(entry) => &entry.id,
            Self::Leaf(entry) => &entry.id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::Message(entry) => entry.parent_id.as_deref(),
            Self::ToolCall(entry) => entry.parent_id.as_deref(),
            Self::ThinkingLevelChange(entry) => entry.parent_id.as_deref(),
            Self::ModelChange(entry) => entry.parent_id.as_deref(),
            Self::ActiveToolsChange(entry) => entry.parent_id.as_deref(),
            Self::Compaction(entry) => entry.parent_id.as_deref(),
            Self::BranchSummary(entry) => entry.parent_id.as_deref(),
            Self::Custom(entry) => entry.parent_id.as_deref(),
            Self::CustomMessage(entry) => entry.parent_id.as_deref(),
            Self::Label(entry) => entry.parent_id.as_deref(),
            Self::SessionInfo(entry) => entry.parent_id.as_deref(),
            Self::Leaf(entry) => entry.parent_id.as_deref(),
        }
    }

    pub fn leaf_target_id(&self) -> Option<&str> {
        match self {
            Self::Leaf(entry) => entry.target_id.as_deref(),
            Self::ToolCall(entry) => Some(&entry.id),
            _ => Some(self.id()),
        }
    }

    pub fn timestamp(&self) -> &str {
        match self {
            Self::Message(e) => &e.timestamp,
            Self::ToolCall(e) => &e.timestamp,
            Self::ThinkingLevelChange(e) => &e.timestamp,
            Self::ModelChange(e) => &e.timestamp,
            Self::ActiveToolsChange(e) => &e.timestamp,
            Self::Compaction(e) => &e.timestamp,
            Self::BranchSummary(e) => &e.timestamp,
            Self::Custom(e) => &e.timestamp,
            Self::CustomMessage(e) => &e.timestamp,
            Self::Label(e) => &e.timestamp,
            Self::SessionInfo(e) => &e.timestamp,
            Self::Leaf(e) => &e.timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_entry_fails_closed_without_runtime_identity() {
        let value = serde_json::json!({
            "type": "message",
            "id": "message-1",
            "parentId": null,
            "timestamp": "1",
            "message": {
                "role": "user",
                "content": "hello"
            }
        });
        assert!(serde_json::from_value::<SessionTreeEntry>(value).is_err());
    }
}
