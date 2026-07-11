//! Tool execution persistence and tool-set registration.

use std::sync::Arc;

use orchd::integration::PersistSink;
use orchd::testing::CollectingPersistSink;
use piko_protocol::agents::HostTaskContext;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::{CannedResponse, CannedToolCall, FauxProvider};
use crate::runtime::{
    test_agent_spec, test_config, test_supervisor, TEST_STREAM_TIMEOUT, run_test_stream,
};
use crate::session_output::collect_test_events;

#[tokio::test]
async fn test_run_with_host_context_emits_tool_result_commit_event() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "need a tool",
        vec![CannedToolCall {
            id: "call_missing".to_string(),
            name: "missing_tool".to_string(),
            arguments: serde_json::json!({"path": "nope"}),
        }],
    ))
    .await;
    faux.push_text("done after tool").await;

    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("tool-commit")).await;

    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;

    let stream = run_test_stream(
        &core,
        "use tool",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("tool-commit".to_string()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_tool".to_string())),
            ..Default::default()
        }),
    )
    .await;

    let _events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;

    let commits = sink.messages();
    assert!(commits.iter().any(|commit| {
        commit.session_id == "session_tool"
            && matches!(
                &commit.message,
                piko_protocol::Message::ToolCall { id, .. } if id == "call_missing"
            )
    }));
    assert!(commits.iter().any(|commit| {
        commit.session_id == "session_tool"
            && matches!(
                &commit.message,
                piko_protocol::Message::ToolResult {
                    tool_call_id,
                    is_error,
                    ..
                } if tool_call_id == "call_missing" && is_error == &Some(true)
            )
    }));
}

#[tokio::test]
async fn test_register_and_unregister_tool_set() {
    let config = test_config();
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let tool_set = piko_protocol::tools::ToolSet {
        id: "test-tools".into(),
        name: "Test Tools".into(),
        description: None,
        tools: vec![],
        policy: None,
        metadata: None,
    };

    core.register_tool_set(tool_set).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.tool_sets.contains_key("test-tools"));

    core.unregister_tool_set("test-tools").await;

    let snapshot2 = core.snapshot().await;
    assert!(!snapshot2.tool_sets.contains_key("test-tools"));
    assert_eq!(snapshot2.tool_sets.get("test-tools"), None);
}
