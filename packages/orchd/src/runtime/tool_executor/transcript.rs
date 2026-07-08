use crate::domain::model::transcript::{ContentBlock, Message, TranscriptManager};
use crate::domain::tools::result::ToolExecResult;
use crate::runtime::types::ToolCallItem;

pub(super) fn append_tool_value(
    transcript: &mut TranscriptManager,
    tc: &ToolCallItem,
    value: serde_json::Value,
    is_error: bool,
) -> Message {
    let text = if value.is_string() {
        value.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&value).unwrap_or_default()
    };
    let msg = Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text { text }],
        details: Some(value),
        is_error: Some(is_error),
        timestamp: None,
    };
    transcript.push_message(msg.clone());
    msg
}

pub(super) fn append_tool(
    transcript: &mut TranscriptManager,
    tc: &ToolCallItem,
    result: &ToolExecResult,
) -> Message {
    let msg = if result.ok {
        let text = match &result.value {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            None => String::new(),
        };
        Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text }],
            details: result.value.clone(),
            is_error: Some(false),
            timestamp: None,
        }
    } else {
        let msg = result
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "Unknown error".into());
        Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text: msg }],
            details: result
                .error
                .as_ref()
                .map(|e| serde_json::to_value(e).unwrap_or_default()),
            is_error: Some(true),
            timestamp: None,
        }
    };
    transcript.push_message(msg.clone());
    msg
}

pub(super) fn append_tool_err(
    transcript: &mut TranscriptManager,
    tc: &ToolCallItem,
    error: &str,
) -> Message {
    let msg = Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text {
            text: format!("Tool error: {error}"),
        }],
        details: Some(serde_json::json!({"error": error})),
        is_error: Some(true),
        timestamp: None,
    };
    transcript.push_message(msg.clone());
    msg
}
