use super::*;

#[tokio::test]
async fn safety_auto_approves_in_roots_write_one_shot_without_grant() {
    let runner = safety_runner(None).await;
    let roots = Some(vec!["/workspace".into()]);

    let first = runner
        .request_tool_approval(write_request(
            "s1",
            "edit",
            "a1",
            "/workspace/src/lib.rs",
            roots.clone(),
        ))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a constrained write"
    );

    // One-shot: an identical second call is assessed again (and accepted
    // again) rather than served from a store grant.
    let second = runner
        .request_tool_approval(write_request(
            "s1",
            "write",
            "a2",
            "/workspace/notes.md",
            roots,
        ))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_rejects_out_of_roots_write_with_reason() {
    let runner = safety_runner(None).await;
    let decision = runner
        .request_tool_approval(write_request(
            "s1",
            "write",
            "a1",
            "/Users/me/.ssh/authorized_keys",
            Some(vec!["/workspace".into()]),
        ))
        .await;
    match decision {
        ToolApprovalDecision::SafetyRejected { reason } => {
            assert!(reason.contains("/Users/me/.ssh/authorized_keys"));
        }
        other => panic!("expected SafetyRejected, got {other:?}"),
    }
    assert!(runner.pending_approvals.lock().unwrap().is_empty());
}

#[tokio::test]
async fn safety_without_writable_roots_falls_through_to_user_flow() {
    let runner = safety_runner(None).await;
    let decision = user_flow_resolves(
        &runner,
        write_request("s1", "edit", "a1", "src/lib.rs", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_opt_out_keeps_user_flow_for_in_roots_write() {
    let runner = safety_runner(Some(&SafetySettings {
        auto_approve_workspace_writes: Some(false),
    }))
    .await;
    let decision = user_flow_resolves(
        &runner,
        write_request(
            "s1",
            "edit",
            "a1",
            "/workspace/src/lib.rs",
            Some(vec!["/workspace".into()]),
        ),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_never_assesses_non_write_tools() {
    let runner = safety_runner(None).await;
    let mut request = approval_request("s1", "exec_command", "a1");
    request.writable_roots = Some(vec!["/workspace".into()]);
    let decision = user_flow_resolves(&runner, request, "a1").await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}
