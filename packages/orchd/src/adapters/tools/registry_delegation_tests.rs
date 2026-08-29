use super::*;

#[tokio::test]
async fn worker_catalog_hides_delegation_tools_and_routes() {
    let registry = catalog_registry(None).await;
    let mut context = discovery_context(None);
    context.agent_kind = piko_protocol::AgentKind::Worker;
    let (tools, routes) = registry
        .discover_tools(&context)
        .await
        .expect("catalog builds");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert!(!names.contains(&"spawn_agent"));
    assert!(!routes.contains_key("spawn_agent"));
    assert!(names.contains(&"read"));
}

#[tokio::test]
async fn worker_cannot_execute_a_stale_delegation_route() {
    let registry = catalog_registry(None).await;
    let (_, mut routes) = registry
        .discover_tools(&discovery_context(None))
        .await
        .expect("supervisor catalog builds");
    let route = routes
        .remove("spawn_agent")
        .expect("supervisor catalog includes delegation route");

    let mut context = context();
    context.agent_kind = piko_protocol::AgentKind::Worker;
    let record = registry
        .execute_tool(
            &ToolCall {
                id: "stale-delegation".into(),
                name: "spawn_agent".into(),
                arguments: serde_json::json!({ "prompt": "must not run" }),
                partial_json: None,
            },
            &context,
            &route,
            None,
        )
        .await;

    let error = record
        .result
        .error
        .expect("stale delegation route must fail closed");
    assert!(!record.result.ok);
    assert_eq!(error.code, "agent_cannot_spawn_children");
    assert_eq!(error.retryable, Some(false));
}
