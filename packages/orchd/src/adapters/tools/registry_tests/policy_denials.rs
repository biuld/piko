use super::*;

#[tokio::test]
async fn safety_rejected_decision_fails_closed_with_reason() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::SafetyRejected {
        reason: "write target `/Users/me/.ssh/config` is outside the sandbox writable roots".into(),
    }))
    .await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("safety rejection must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "safety_rejected");
    assert_eq!(error.retryable, Some(false));
    assert!(error.message.contains("outside the sandbox writable roots"));
}

#[tokio::test]
async fn permission_denied_decision_fails_closed_with_reason() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::PermissionDenied {
        reason: "command prefix 'rm -rf' is denied by permission policy".into(),
    }))
    .await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("permission denial must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "permission_denied");
    assert_eq!(error.retryable, Some(false));
    assert!(error.message.contains("rm -rf"));
}

#[test]
fn expired_is_never_accepted() {
    assert!(!is_approval_accepted(&ToolApprovalDecision::Expired));
    assert!(!is_approval_accepted(&ToolApprovalDecision::Decline));
    assert!(!is_approval_accepted(
        &ToolApprovalDecision::GuardianDenied { reason: "x".into() }
    ));
    assert!(!is_approval_accepted(
        &ToolApprovalDecision::GuardianUnavailable
    ));
    assert!(!is_approval_accepted(
        &ToolApprovalDecision::SafetyRejected { reason: "x".into() }
    ));
    assert!(!is_approval_accepted(
        &ToolApprovalDecision::PermissionDenied { reason: "x".into() }
    ));
    assert!(is_approval_accepted(&ToolApprovalDecision::Accept));
    assert!(is_approval_accepted(&ToolApprovalDecision::AcceptSession));
}
