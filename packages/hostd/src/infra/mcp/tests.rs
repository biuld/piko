use std::collections::HashMap;
use std::time::Duration;

use piko_orchd_api::{ToolExecutionContext, ToolProvider};

use crate::domain::config::McpServerConfig;

use super::provider::McpProvider;
use super::resource::{
    MCP_RESOURCE_PROVIDER_ID, MCP_RESOURCE_TOOL_NAME, McpResourceProvider, mcp_resource_tool_def,
};

fn fixture_config(name: &str, script: &str) -> McpServerConfig {
    McpServerConfig {
        name: name.into(),
        command: "sh".into(),
        args: vec![
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(script)
                .to_string_lossy()
                .into_owned(),
        ],
        env: HashMap::new(),
        timeout_ms: None,
    }
}

fn execution_context() -> ToolExecutionContext {
    ToolExecutionContext {
        root_input_id: "input-1".into(),
        session_id: "s1".into(),
        agent_instance_id: "root".into(),
        cancellation: None,
        agent_id: "main".into(),
        agent_role: None,
        agent_kind: piko_protocol::AgentKind::Supervisor,
        tool_set_ids: vec![],
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
        context_remaining: None,
    }
}

#[test]
fn test_mcp_server_config_deserialize() {
    let config: McpServerConfig = serde_json::from_str(
        r#"{"name": "filesystem", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"], "env": {}}"#,
    )
    .unwrap();
    assert_eq!(config.name, "filesystem");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args.len(), 3);
}

#[test]
fn test_mcp_server_config_defaults() {
    let config: McpServerConfig =
        serde_json::from_str(r#"{"name": "echo", "command": "cat"}"#).unwrap();
    assert_eq!(config.args, Vec::<String>::new());
    assert_eq!(config.env, HashMap::new());
}

#[test]
fn test_mcp_server_config_timeout_ms_deserialize() {
    let config: McpServerConfig =
        serde_json::from_str(r#"{"name": "echo", "command": "cat", "timeout_ms": 2500}"#).unwrap();
    assert_eq!(config.timeout_ms, Some(2500));
}

#[tokio::test]
async fn provider_connects_and_discovers_tools_and_resources() {
    let provider = McpProvider::connect(
        &fixture_config("fixture", "mcp_server.sh"),
        Duration::from_secs(5),
    )
    .await
    .expect("connect to fixture server");

    assert_eq!(provider.tools.len(), 1);
    assert_eq!(provider.resources.len(), 1);
    assert_eq!(provider.templates.len(), 1);

    let list = provider.list_resources(None).await.expect("list resources");
    assert_eq!(list["server"], "fixture");
    assert_eq!(list["resources"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["templates"].as_array().map(Vec::len), Some(1));

    // Client-side search filter (F-13): substring over uri/name/description.
    let filtered = provider
        .list_resources(Some("notes"))
        .await
        .expect("filtered list");
    assert_eq!(filtered["resources"].as_array().map(Vec::len), Some(1));
    let filtered_none = provider
        .list_resources(Some("zzz"))
        .await
        .expect("empty filtered list");
    assert_eq!(filtered_none["resources"].as_array().map(Vec::len), Some(0));

    let read = provider
        .read_resource("file:///tmp/notes.md")
        .await
        .expect("read resource");
    assert_eq!(read["uri"], "file:///tmp/notes.md");
    assert_eq!(read["text"], "hello from fixture");
}

#[tokio::test]
async fn provider_without_resource_support_still_connects() {
    let provider = McpProvider::connect(
        &fixture_config("no-resources", "mcp_server_no_resources.sh"),
        Duration::from_secs(5),
    )
    .await
    .expect("connect despite missing resource support");

    // Tools work; resources degrade to an empty catalog (not a failure).
    assert_eq!(provider.tools.len(), 1);
    assert!(provider.resources.is_empty());
    assert!(provider.templates.is_empty());
}

#[tokio::test]
async fn provider_connect_times_out_fail_closed() {
    let config = McpServerConfig {
        name: "hang".into(),
        command: "sh".into(),
        args: vec!["-c".into(), "sleep 30".into()],
        env: HashMap::new(),
        timeout_ms: None,
    };
    let started = std::time::Instant::now();
    let result = McpProvider::connect(&config, Duration::from_millis(200)).await;
    assert!(result.is_err(), "expected timeout failure");
    let message = result.expect_err("error message");
    assert!(message.contains("timed out"), "message: {message}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "timeout must bound the connect"
    );
}

#[test]
fn mcp_resource_tool_is_gated_as_mcp_and_read_only() {
    let tool = mcp_resource_tool_def();
    assert_eq!(tool.name, MCP_RESOURCE_TOOL_NAME);
    assert_eq!(
        tool.executor.kind, "mcp",
        "F-18 mcp gate must cover the tool"
    );
    assert_eq!(tool.executor.target, MCP_RESOURCE_PROVIDER_ID);
    assert_eq!(
        tool.approval,
        Some(piko_protocol::tools::ToolApprovalRequirement::Never)
    );
    assert_eq!(tool.metadata.as_ref().and_then(|m| m.read_only), Some(true));
}

#[tokio::test]
async fn mcp_resource_provider_lists_and_reads_and_fails_closed() {
    let provider = McpProvider::connect(
        &fixture_config("files", "mcp_server.sh"),
        Duration::from_secs(5),
    )
    .await
    .expect("connect to fixture server");
    let resource_provider = McpResourceProvider::new(HashMap::from([(
        "files".to_string(),
        std::sync::Arc::new(provider),
    )]));

    let list = resource_provider
        .execute(
            piko_protocol::ToolCall {
                id: "t1".into(),
                name: MCP_RESOURCE_TOOL_NAME.into(),
                arguments: serde_json::json!({ "server": "files" }),
                partial_json: None,
            },
            execution_context(),
        )
        .await;
    assert!(list.ok, "list should succeed");
    assert_eq!(
        list.value
            .as_ref()
            .and_then(|v| v.get("resources"))
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(1)
    );

    let read = resource_provider
        .execute(
            piko_protocol::ToolCall {
                id: "t2".into(),
                name: MCP_RESOURCE_TOOL_NAME.into(),
                arguments: serde_json::json!({
                    "server": "files",
                    "uri": "file:///tmp/notes.md"
                }),
                partial_json: None,
            },
            execution_context(),
        )
        .await;
    assert!(read.ok, "read should succeed");
    assert_eq!(
        read.value.as_ref().and_then(|v| v.get("text")),
        Some(&serde_json::json!("hello from fixture"))
    );

    // Unknown server fails closed with a distinct non-retryable error.
    let unknown = resource_provider
        .execute(
            piko_protocol::ToolCall {
                id: "t3".into(),
                name: MCP_RESOURCE_TOOL_NAME.into(),
                arguments: serde_json::json!({ "server": "missing" }),
                partial_json: None,
            },
            execution_context(),
        )
        .await;
    assert!(!unknown.ok);
    assert_eq!(
        unknown.error.as_ref().map(|e| e.code.as_str()),
        Some("mcp_resource_server_unknown")
    );
    assert_eq!(
        unknown.error.as_ref().and_then(|e| e.retryable),
        Some(false)
    );
}
