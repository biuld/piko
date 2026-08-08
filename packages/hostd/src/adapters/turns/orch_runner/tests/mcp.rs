use super::*;

#[tokio::test]
async fn mcp_approval_template_prompt_reaches_the_user_snapshot() {
    let runner = mcp_template_runner(std::collections::HashMap::from([
        (
            "github/create_issue".into(),
            "This creates a GitHub issue in the configured repository.".into(),
        ),
        (
            "delete_resource".into(),
            "Delete {tool} on {server} with args {args}".into(),
        ),
    ]))
    .await;

    // server/tool template renders into the pending snapshot prompt.
    let request = ToolApprovalRequest {
        tool_entity_id: "mcp-1".into(),
        call_id: "call-mcp-1".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("github".into()),
        tool_name: "create_issue".into(),
        tool_args: serde_json::json!({ "title": "x" }),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending =
        tokio::spawn(async move { runner_for_spawn.request_tool_approval(request).await });
    let snapshot_prompt = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            // Read the snapshot in its own statement so the std Mutex guard
            // drops before the await below (single-thread runtime must not
            // hold a sync lock across .await).
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-1")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            if let Some(prompt) = prompt {
                break prompt;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("snapshot appears in time");
    assert_eq!(
        snapshot_prompt.as_deref(),
        Some("This creates a GitHub issue in the configured repository.")
    );
    let responded = runner
        .respond_approval("mcp-1", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // Bare `tool` fallback substitutes placeholders.
    let bare = ToolApprovalRequest {
        tool_entity_id: "mcp-2".into(),
        call_id: "call-mcp-2".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("github".into()),
        tool_name: "delete_resource".into(),
        tool_args: serde_json::json!({ "id": 7 }),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending = tokio::spawn(async move { runner_for_spawn.request_tool_approval(bare).await });
    let bare_prompt = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-2")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            if let Some(prompt) = prompt {
                break prompt;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("snapshot appears in time");
    let bare_prompt = bare_prompt.expect("bare tool template resolves");
    assert!(
        bare_prompt.contains("Delete delete_resource on github"),
        "{bare_prompt}"
    );
    assert!(bare_prompt.contains("{\"id\":7}"), "{bare_prompt}");
    let responded = runner
        .respond_approval("mcp-2", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // A non-MCP provider is never matched by a bare `tool` key.
    let non_mcp = ToolApprovalRequest {
        tool_entity_id: "mcp-3".into(),
        call_id: "call-mcp-3".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("workspace".into()),
        tool_name: "delete_resource".into(),
        tool_args: serde_json::json!({}),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending =
        tokio::spawn(async move { runner_for_spawn.request_tool_approval(non_mcp).await });
    let prompt_absent = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-3")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            match prompt {
                Some(prompt) => break prompt,
                None => {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }
    })
    .await
    .expect("snapshot appears in time");
    assert!(
        prompt_absent.is_none(),
        "bare tool keys must not match non-MCP tools"
    );
    let responded = runner
        .respond_approval("mcp-3", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn mcp_statuses_reports_configured_servers() {
    let runner = mcp_template_runner(std::collections::HashMap::new()).await;
    let statuses = runner.mcp_statuses().await;
    // The fixture `echo` server cannot speak JSON-RPC, so the entry exists
    // but reports disconnected with the connect error.
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name, "github");
    assert!(!statuses[0].connected);
    assert!(statuses[0].error.is_some());
}
