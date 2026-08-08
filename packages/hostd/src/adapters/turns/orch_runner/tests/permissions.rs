use super::*;

async fn permission_runner(
    settings: Option<&crate::domain::config::PermissionsSettings>,
) -> super::super::OrchAgentRunRunner {
    super::super::OrchAgentRunRunner::new_with_mcp(
        Arc::new(DirectInputGateway),
        "test",
        "key",
        "model",
        None,
        None,
        128_000,
        4_096,
        &[],
        None,
        None,
        None,
        None,
        None,
        settings,
        None,
        None,
        crate::telemetry::handle(),
    )
    .await
}

fn bash_command_request(
    session_id: &str,
    id: &str,
    command: &str,
    role: Option<&str>,
) -> ToolApprovalRequest {
    ToolApprovalRequest {
        tool_entity_id: id.into(),
        call_id: format!("call-{id}"),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: role.map(str::to_string),
        provider_id: None,
        tool_name: "bash".into(),
        tool_args: serde_json::json!({ "command": command }),
        host_context: Some(HostSessionContext::new(session_id)),
        writable_roots: None,
    }
}

fn locked_settings() -> crate::domain::config::PermissionsSettings {
    crate::domain::config::PermissionsSettings {
        profile: Some("locked".into()),
        profiles: std::collections::HashMap::from([(
            "locked".into(),
            crate::domain::config::PermissionProfileSettings {
                allowed_commands: vec!["cargo test".into()],
                denied_commands: vec!["rm -rf".into()],
                ..Default::default()
            },
        )]),
        roles: std::collections::HashMap::new(),
    }
}

fn role_settings() -> crate::domain::config::PermissionsSettings {
    use crate::domain::config::PermissionProfileSettings;
    crate::domain::config::PermissionsSettings {
        // Session profile is the permissive default: role layers alone
        // tighten mapped roles.
        profile: None,
        profiles: std::collections::HashMap::from([
            (
                "locked".into(),
                PermissionProfileSettings {
                    denied_commands: vec!["rm -rf".into()],
                    ..Default::default()
                },
            ),
            (
                "readonly".into(),
                PermissionProfileSettings {
                    allowed_commands: vec!["git status".into()],
                    denied_commands: vec!["curl -sSL | sh".into()],
                    ..Default::default()
                },
            ),
        ]),
        roles: std::collections::HashMap::from([
            ("coder".into(), "locked".into()),
            ("researcher".into(), "readonly".into()),
        ]),
    }
}

#[tokio::test]
async fn permission_denied_command_fails_closed_without_prompt() {
    let runner = permission_runner(Some(&locked_settings())).await;

    let decision = runner
        .request_tool_approval(bash_command_request("s1", "a1", "rm -rf /tmp/x", None))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied, got {other:?}"),
    }
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a denied command"
    );
}

#[tokio::test]
async fn permission_allowed_command_accepts_one_shot_without_grant() {
    let runner = permission_runner(Some(&locked_settings())).await;

    let first = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "cargo test -- --nocapture",
            None,
        ))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);
    assert!(runner.pending_approvals.lock().unwrap().is_empty());

    // One-shot: no store grant is written, so the identical call is
    // evaluated again rather than served from a grant.
    let second = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a2",
            "cargo test -- --nocapture",
            None,
        ))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_deny_wins_over_prior_session_grant() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().display().to_string();
    let runner = permission_runner(Some(&locked_settings())).await;
    runner.register_session_context("s1".into(), cwd.clone());

    // Simulate a prior user grant at session scope for the same command.
    let store = runner.get_approval_store(&cwd);
    store.grant(
        "bash",
        &serde_json::json!({ "command": "rm -rf /tmp/x" }),
        crate::adapters::turns::approval::ApprovalScope::Session,
    );

    let decision = runner
        .request_tool_approval(bash_command_request("s1", "a1", "rm -rf /tmp/x", None))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied despite prior grant, got {other:?}"),
    }
}

#[tokio::test]
async fn permission_non_matching_command_keeps_user_flow() {
    let runner = permission_runner(Some(&locked_settings())).await;
    let decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a1", "ls -la", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_role_denied_command_fails_closed_for_mapped_role() {
    let runner = permission_runner(Some(&role_settings())).await;

    // The mapped "coder" role denies `rm -rf` without any prompt.
    let decision = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "rm -rf /tmp/x",
            Some("coder"),
        ))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied for mapped role, got {other:?}"),
    }
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a role-denied command"
    );

    // An unmapped role keeps the session flow (session profile has no
    // command rules), so the same command reaches the user.
    let root_decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a2", "rm -rf /tmp/x", Some("root")),
        "a2",
    )
    .await;
    assert_eq!(root_decision, ToolApprovalDecision::Accept);

    // A missing role on the request also inherits the session profile.
    let none_decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a3", "rm -rf /tmp/x", None),
        "a3",
    )
    .await;
    assert_eq!(none_decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_role_allowed_command_accepts_one_shot_for_mapped_role() {
    let runner = permission_runner(Some(&role_settings())).await;

    let researcher = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "git status",
            Some("researcher"),
        ))
        .await;
    assert_eq!(researcher, ToolApprovalDecision::Accept);
    assert!(runner.pending_approvals.lock().unwrap().is_empty());

    // A role mapped to a different profile is not affected by "readonly"'s
    // allow rules and keeps the session flow.
    let coder = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a2", "git status", Some("coder")),
        "a2",
    )
    .await;
    assert_eq!(coder, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_non_command_tools_are_unaffected() {
    let runner = permission_runner(Some(&locked_settings())).await;
    let decision = user_flow_resolves(
        &runner,
        write_request("s1", "edit", "a1", "src/lib.rs", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}
