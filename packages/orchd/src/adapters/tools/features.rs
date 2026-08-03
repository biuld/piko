//! F-18 managed feature gating: canonical tool → feature mapping.
//!
//! The canonical key list lives in hostd (`domain/features`); this module
//! maps piko tool definitions/names to those keys. MCP tools are identified
//! by executor kind (`"mcp"`) because their names are server-defined and
//! cannot be classified by name alone.

use std::collections::HashMap;

use crate::domain::tools::definition::ToolDef;

/// Canonical feature for a tool definition.
pub fn feature_for_tool(tool: &ToolDef) -> Option<&'static str> {
    if tool.executor.kind == "mcp" {
        return Some("mcp");
    }
    feature_for_tool_name(&tool.name)
}

/// Canonical feature for a tool name. MCP tools are not classified here
/// (server-defined names); use [`feature_for_tool`] when a `ToolDef` is
/// available.
pub fn feature_for_tool_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "read" | "edit" | "write" => "workspace",
        "bash" => "bash",
        "process" => "process",
        "environment" => "environment",
        "get_context_remaining" | "new_context_window" => "context",
        "todo_read" | "todo_write" => "todo",
        "spawn_agent"
        | "spawn_agent_detached"
        | "send_agent_message"
        | "collect_agent_reports"
        | "close_agent"
        | "reopen_agent"
        | "followup_task"
        | "interrupt_agent"
        | "list_agents"
        | "wait_agent" => "multi-agent",
        "ask_user" | "request_user_input" => "user-interaction",
        _ => return None,
    })
}

/// Whether a tool passes the feature gate for a resolved feature set.
/// A tool is enabled when its feature is absent from the map or true.
/// Unmapped tools are always enabled (today's behavior).
pub fn feature_enabled(features: Option<&HashMap<String, bool>>, tool: &ToolDef) -> bool {
    let Some(key) = feature_for_tool(tool) else {
        return true;
    };
    features
        .and_then(|map| map.get(key))
        .copied()
        .unwrap_or(true)
}

/// The feature key that disables a tool *name* for the direct-call error
/// path. Returns `None` for ungated tools and enabled features.
pub fn disabled_feature_for_tool_name(
    features: Option<&HashMap<String, bool>>,
    name: &str,
) -> Option<&'static str> {
    let key = feature_for_tool_name(name)?;
    let enabled = features
        .and_then(|map| map.get(key))
        .copied()
        .unwrap_or(true);
    if enabled { None } else { Some(key) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, executor_kind: &str) -> ToolDef {
        ToolDef {
            name: name.into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "test"),
            description: String::new(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: executor_kind.into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        }
    }

    #[test]
    fn catalog_tools_map_to_features() {
        assert_eq!(feature_for_tool(&tool("read", "native")), Some("workspace"));
        assert_eq!(feature_for_tool(&tool("bash", "native")), Some("bash"));
        assert_eq!(
            feature_for_tool(&tool("process", "native")),
            Some("process")
        );
        assert_eq!(
            feature_for_tool(&tool("wait_agent", "native")),
            Some("multi-agent")
        );
    }

    #[test]
    fn mcp_tools_are_identified_by_executor_kind() {
        assert_eq!(
            feature_for_tool(&tool("arbitrary_name", "mcp")),
            Some("mcp")
        );
        // Name-only classification cannot identify server-defined MCP tools.
        assert_eq!(feature_for_tool_name("arbitrary_name"), None);
    }

    #[test]
    fn unknown_tools_are_ungated() {
        assert_eq!(feature_for_tool(&tool("future_tool", "native")), None);
        assert!(feature_enabled(
            Some(&HashMap::new()),
            &tool("future_tool", "native")
        ));
    }

    #[test]
    fn feature_gate_respects_resolved_map() {
        let mut features = HashMap::new();
        features.insert("process".to_string(), false);
        assert!(!feature_enabled(
            Some(&features),
            &tool("process", "native")
        ));
        assert!(feature_enabled(Some(&features), &tool("bash", "native")));
        assert_eq!(
            disabled_feature_for_tool_name(Some(&features), "process"),
            Some("process")
        );
        assert_eq!(
            disabled_feature_for_tool_name(Some(&features), "bash"),
            None
        );
    }

    #[test]
    fn absent_feature_map_keeps_everything_enabled() {
        assert!(feature_enabled(None, &tool("process", "native")));
        assert_eq!(disabled_feature_for_tool_name(None, "process"), None);
    }
}
