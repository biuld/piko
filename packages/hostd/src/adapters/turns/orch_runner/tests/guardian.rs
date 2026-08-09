use super::*;

#[tokio::test]
async fn guardian_allow_executes_one_shot_without_store_grant() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            Ok(GuardianDecision {
                allow: true,
                reason: "build check".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let first = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a1"))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);

    // One-shot semantics: an identical second call is reviewed again rather
    // than served from a session/workspace/permanent grant.
    let second = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a2"))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn guardian_deny_fails_closed_and_breaker_escalates_to_user_then_resets() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            Ok(GuardianDecision {
                allow: false,
                reason: "outside workspace".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 2).await;

    let first = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a1"))
        .await;
    assert!(matches!(
        first,
        ToolApprovalDecision::GuardianDenied { reason } if reason == "outside workspace"
    ));

    let second = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a2"))
        .await;
    assert!(matches!(
        second,
        ToolApprovalDecision::GuardianDenied { .. }
    ));

    // Third request: breaker tripped, so the user flow owns the decision.
    let runner_for_spawn = runner.clone();
    let third = tokio::spawn(async move {
        runner_for_spawn
            .request_tool_approval(approval_request("s1", "exec_command", "a3"))
            .await
    });
    for _ in 0..200 {
        if runner.pending_approvals.lock().unwrap().contains_key("a3") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        runner.pending_approvals.lock().unwrap().contains_key("a3"),
        "tripped guardian must escalate to the user flow"
    );
    let responded = runner
        .respond_approval("a3", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), third)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // A user decision reset the breaker: the loop reviews again.
    let fourth = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a4"))
        .await;
    assert!(matches!(
        fourth,
        ToolApprovalDecision::GuardianDenied { .. }
    ));
}

#[tokio::test]
async fn guardian_failure_fails_closed_without_running() {
    let review: GuardianReviewCallback =
        Arc::new(|_, _| Box::pin(async { Err::<GuardianDecision, _>("model down".into()) }));
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let decision = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a1"))
        .await;
    assert_eq!(decision, ToolApprovalDecision::GuardianUnavailable);
}

#[tokio::test]
async fn guardian_timeout_fails_closed() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(GuardianDecision {
                allow: true,
                reason: "late".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let decision = runner
        .request_tool_approval(approval_request("s1", "exec_command", "a1"))
        .await;
    assert_eq!(decision, ToolApprovalDecision::GuardianUnavailable);
}
