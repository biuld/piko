use std::path::PathBuf;
use std::sync::Arc;

use piko_sandbox::exec::ShellSnapshot;
use piko_sandbox::exec::process::ProcessManager;
use piko_sandbox::policy::EffectivePermissions;

use super::exec_handlers::{execute_exec_command, execute_write_stdin};
use crate::domain::tools::call::ToolCall;
use crate::ports::tool_provider::ToolExecutionContext;

fn policy(root: PathBuf) -> EffectivePermissions {
    EffectivePermissions {
        version: 1,
        read_roots: vec![root.clone()],
        write_roots: vec![root],
        scratch_roots: vec![],
        denied_read_roots: vec![],
        denied_write_roots: vec![],
        network: false.into(),
    }
}

fn shell(cwd: PathBuf) -> ShellSnapshot {
    ShellSnapshot {
        shell_path: "bash".into(),
        cwd,
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
    }
}

fn context() -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: "session".into(),
        agent_instance_id: "agent".into(),
        root_input_id: "execution".into(),
        cancellation: None,
        agent_id: "root".into(),
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

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "call".into(),
        name: name.into(),
        arguments,
        partial_json: None,
    }
}

#[tokio::test]
async fn full_shell_and_nonzero_exit_are_normal_results() {
    let temp = tempfile::tempdir().unwrap();
    let manager = Arc::new(ProcessManager::new());
    let command = call(
        "exec_command",
        serde_json::json!({
            "cmd": "cd .\nPIKO_VALUE=$(printf piko)\ncat <<EOF | tr a-z A-Z\n$PIKO_VALUE\nEOF\nexit 7",
            "sandbox_permissions": "require_escalated",
            "justification": "test the shell contract",
            "yield_time_ms": 2_000
        }),
    );
    let result = execute_exec_command(
        &manager,
        &policy(temp.path().to_path_buf()),
        &shell(temp.path().to_path_buf()),
        &command,
        &context(),
    )
    .await;
    assert!(result.ok, "{result:?}");
    let value = result.value.unwrap();
    assert_eq!(value["state"], "exited");
    assert_eq!(value["exit_code"], 7);
    assert_eq!(value["output"], "PIKO\n");
    assert!(value.get("session_id").is_none());
}

#[tokio::test]
async fn exit_one_and_command_not_found_are_completed_results() {
    let temp = tempfile::tempdir().unwrap();
    let manager = Arc::new(ProcessManager::new());
    for (cmd, expected) in [("exit 1", 1), ("piko-command-that-does-not-exist", 127)] {
        let command = call(
            "exec_command",
            serde_json::json!({
                "cmd": cmd,
                "sandbox_permissions": "require_escalated",
                "justification": "test process exit semantics",
                "yield_time_ms": 2_000
            }),
        );
        let result = execute_exec_command(
            &manager,
            &policy(temp.path().to_path_buf()),
            &shell(temp.path().to_path_buf()),
            &command,
            &context(),
        )
        .await;
        assert!(result.ok, "{result:?}");
        assert_eq!(result.value.unwrap()["exit_code"], expected);
    }
}

#[tokio::test]
async fn running_command_continues_through_write_stdin() {
    let temp = tempfile::tempdir().unwrap();
    let manager = Arc::new(ProcessManager::new());
    let command = call(
        "exec_command",
        serde_json::json!({
            "cmd": "printf start; sleep 0.2; printf end",
            "sandbox_permissions": "require_escalated",
            "justification": "test session continuation",
            "yield_time_ms": 10
        }),
    );
    let started = execute_exec_command(
        &manager,
        &policy(temp.path().to_path_buf()),
        &shell(temp.path().to_path_buf()),
        &command,
        &context(),
    )
    .await;
    let started = started.value.unwrap();
    assert_eq!(started["state"], "running");
    let session_id = started["session_id"].as_str().unwrap();

    let poll = call(
        "write_stdin",
        serde_json::json!({ "session_id": session_id, "yield_time_ms": 1_000 }),
    );
    let completed = execute_write_stdin(&manager, &poll).await;
    assert!(completed.ok, "{completed:?}");
    let completed = completed.value.unwrap();
    assert_eq!(completed["state"], "exited");
    assert_eq!(completed["exit_code"], 0);
    assert!(completed["output"].as_str().unwrap().contains("end"));
    assert!(completed.get("session_id").is_none());
}

#[tokio::test]
async fn workdir_cannot_rebase_relative_policy_roots() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let manager = ProcessManager::new();
    let command = call(
        "exec_command",
        serde_json::json!({
            "cmd": "pwd",
            "workdir": outside.path(),
            "sandbox_permissions": "use_default"
        }),
    );
    let result = execute_exec_command(
        &manager,
        &policy(PathBuf::from(".")),
        &shell(workspace.path().to_path_buf()),
        &command,
        &context(),
    )
    .await;
    assert!(!result.ok);
    assert_eq!(result.error.unwrap().code, "sandbox_denied");
}
