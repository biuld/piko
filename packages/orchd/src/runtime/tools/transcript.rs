use crate::domain::tools::call::ToolCallItem;
use crate::domain::tools::result::ToolExecResult;
use crate::domain::transcript::{ContentBlock, Message};

pub(crate) fn build_tool_result(tc: &ToolCallItem, result: &ToolExecResult) -> Message {
    if result.ok {
        let visible_value = result.value.as_ref().map(model_visible_tool_value);
        let text = match &visible_value {
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
    }
}

fn model_visible_tool_value(value: &serde_json::Value) -> serde_json::Value {
    let mut visible = value.clone();
    if let Some(object) = visible.as_object_mut() {
        object.remove(crate::adapters::tools::FILE_CHANGE_DETAILS_KEY);
    }
    visible
}

pub(crate) fn build_tool_error(tc: &ToolCallItem, error: &str) -> Message {
    Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text {
            text: format!("Tool error: {error}"),
        }],
        details: Some(serde_json::json!({"error": error})),
        is_error: Some(true),
        timestamp: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::tools::call::ToolCallItem;
    use crate::domain::transcript::{ContentBlock, Message};
    use piko_orchd_api::ToolExecResult;

    use super::build_tool_result;

    #[test]
    fn file_change_details_are_durable_but_not_model_visible() {
        let call = ToolCallItem {
            tool_call_index: 0,
            content_index: 0,
            id: "call-1".into(),
            name: "edit".into(),
            arguments: serde_json::json!({}),
        };
        let result = ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({
                "edited": true,
                "_pikoFileChange": {
                    "path": "a.rs",
                    "before": "old",
                    "after": "new"
                }
            })),
            error: None,
        };

        let Message::ToolResult {
            content, details, ..
        } = build_tool_result(&call, &result)
        else {
            panic!("expected tool result");
        };
        assert!(matches!(
            &content[0],
            ContentBlock::Text { text }
                if text.contains("edited")
                    && !text.contains("_pikoFileChange")
                    && !text.contains("old")
        ));
        assert!(
            details
                .as_ref()
                .and_then(|value| value.get("_pikoFileChange"))
                .is_some()
        );
    }
}
