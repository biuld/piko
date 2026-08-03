// ---- `process` tool: long-lived PTY processes ----
//
// F-08 slice 2: a `ProcessManager` (owned by the WorkspaceToolProvider)
// keeps started processes alive across tool calls. Actions: `start`
// (with optional cwd/env overrides), `write` (stdin), `read` (incremental
// output), `stop` (group SIGTERM → SIGKILL), and `list`.

use std::time::Duration;

use piko_sandbox::exec::process::{DEFAULT_MAX_OUTPUT_BYTES, ProcessManager};
use piko_sandbox::exec::{ShellSnapshot, SpawnConfig};
use piko_sandbox::policy::Policy;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutorRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;

const STOP_GRACE: Duration = Duration::from_secs(2);

pub(super) fn process_tool_def() -> ToolDef {
    ToolDef {
        name: "process".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/process"),
        description:
            "Manage long-lived processes: start (PTY, optional cwd/env), write (stdin), read (incremental output), stop, list."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "write", "read", "stop", "list"]
                },
                "command": { "type": "string", "description": "Command for start" },
                "cwd": { "type": "string", "description": "Working directory override for start" },
                "env": {
                    "type": "object",
                    "description": "Environment variable overrides for start"
                },
                "processId": { "type": "string", "description": "Process id from start" },
                "data": { "type": "string", "description": "Bytes to write to stdin" }
            },
            "required": ["action"]
        }),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: "process".into(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: Some(vec![
            ToolCapability::Process,
            ToolCapability::WorkspaceRead,
            ToolCapability::WorkspaceWrite,
        ]),
        approval: Some(ToolApprovalRequirement::OnRequest),
        metadata: None,
    }
}

pub(super) async fn execute_process_tool(
    processes: &ProcessManager,
    policy: &Policy,
    shell: &ShellSnapshot,
    os_sandbox: bool,
    call: &ToolCall,
    _ctx: &ToolExecutionContext,
) -> ToolExecResult {
    let arguments = &call.arguments;
    let action = arguments
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match action {
        "start" => start(processes, policy, shell, os_sandbox, arguments).await,
        "write" => write(processes, arguments).await,
        "read" => read(processes, arguments),
        "stop" => stop(processes, arguments).await,
        "list" => list(processes),
        _ => ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "invalid_args".into(),
                message: format!("Unknown process action: {action}"),
                retryable: Some(false),
            }),
        },
    }
}

async fn start(
    processes: &ProcessManager,
    policy: &Policy,
    shell: &ShellSnapshot,
    os_sandbox: bool,
    arguments: &serde_json::Value,
) -> ToolExecResult {
    let command = arguments
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if command.is_empty() {
        return ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "invalid_args".into(),
                message: "command is required for process start".into(),
                retryable: Some(false),
            }),
        };
    }

    // Apply optional cwd/env overrides on top of the shell snapshot.
    let mut snapshot = shell.clone();
    if let Some(cwd) = arguments.get("cwd").and_then(|v| v.as_str()) {
        snapshot.cwd = std::path::PathBuf::from(cwd);
    }
    if let Some(env) = arguments.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env {
            if let Some(value) = value.as_str() {
                if let Some(entry) = snapshot.env.iter_mut().find(|(k, _)| k == key) {
                    entry.1 = value.to_string();
                } else {
                    snapshot.env.push((key.clone(), value.to_string()));
                }
            }
        }
    }

    if let Err(e) = policy.validate_command(command, &snapshot.cwd) {
        return ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "policy_violation".into(),
                message: e.to_string(),
                retryable: Some(false),
            }),
        };
    }

    let config = SpawnConfig {
        command: command.to_string(),
        shell: snapshot,
        policy: os_sandbox.then(|| policy.clone()),
        timeout: None,
        cancel: None,
        kill_grace: Duration::from_secs(2),
        max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
    };

    match processes.start(config).await {
        Ok(process) => ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({
                "processId": process.id(),
                "pid": process.pid(),
            })),
            error: None,
        },
        Err(e) => ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "exec_error".into(),
                message: e.to_string(),
                retryable: Some(false),
            }),
        },
    }
}

async fn write(processes: &ProcessManager, arguments: &serde_json::Value) -> ToolExecResult {
    let Some(process_id) = arguments.get("processId").and_then(|v| v.as_str()) else {
        return missing_process_id();
    };
    let Some(process) = processes.get(process_id) else {
        return unknown_process(process_id);
    };
    let data = arguments.get("data").and_then(|v| v.as_str()).unwrap_or("");

    match process.write_stdin(data.as_bytes()).await {
        Ok(bytes_written) => ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({ "bytesWritten": bytes_written })),
            error: None,
        },
        Err(e) => ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "io_error".into(),
                message: format!("write_stdin failed: {e}"),
                retryable: Some(false),
            }),
        },
    }
}

fn read(processes: &ProcessManager, arguments: &serde_json::Value) -> ToolExecResult {
    let Some(process_id) = arguments.get("processId").and_then(|v| v.as_str()) else {
        return missing_process_id();
    };
    let Some(process) = processes.get(process_id) else {
        return unknown_process(process_id);
    };
    let chunk = process.try_read_output();
    ToolExecResult {
        ok: true,
        value: Some(serde_json::json!({
            "output": String::from_utf8_lossy(&chunk.bytes),
            "bytes": chunk.bytes.len(),
            "truncated": chunk.truncated,
            "exited": chunk.exited,
            "exitCode": chunk.status.and_then(|s| s.code),
            "signal": chunk.status.and_then(|s| s.signal),
        })),
        error: None,
    }
}

async fn stop(processes: &ProcessManager, arguments: &serde_json::Value) -> ToolExecResult {
    let Some(process_id) = arguments.get("processId").and_then(|v| v.as_str()) else {
        return missing_process_id();
    };
    match processes.stop(process_id, STOP_GRACE).await {
        Some(status) => ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({
                "stopped": true,
                "exitCode": status.code,
                "signal": status.signal,
            })),
            error: None,
        },
        None => unknown_process(process_id),
    }
}

fn list(processes: &ProcessManager) -> ToolExecResult {
    let ids = processes.list();
    ToolExecResult {
        ok: true,
        value: Some(serde_json::json!({ "processIds": ids })),
        error: None,
    }
}

fn missing_process_id() -> ToolExecResult {
    ToolExecResult {
        ok: false,
        value: None,
        error: Some(ToolExecError {
            code: "invalid_args".into(),
            message: "processId is required".into(),
            retryable: Some(false),
        }),
    }
}

fn unknown_process(process_id: &str) -> ToolExecResult {
    ToolExecResult {
        ok: false,
        value: None,
        error: Some(ToolExecError {
            code: "unknown_process".into(),
            message: format!("No such process: {process_id}"),
            retryable: Some(false),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_orchd_api::tools::ToolExecutionContext;
    use piko_protocol::messages::ToolCall;

    fn policy() -> Policy {
        Policy {
            version: 1,
            read: vec![std::path::PathBuf::from(".")],
            write: vec![std::path::PathBuf::from(".")],
            deny: vec![],
            allowed_commands: vec!["cat".into(), "sleep".into()],
            allow_network: false,
        }
    }

    fn snapshot(cwd: std::path::PathBuf) -> ShellSnapshot {
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
            execution_id: "exec".into(),
            cancellation: None,
            agent_id: "root".into(),
            tool_set_ids: vec![],
            turn_index: None,
            event_seq: None,
            next_event_seq: None,
            parent_message_id: None,
            content_index: None,
            tool_call_index: Some(0),
            tool_entity_id: Some("entity".into()),
            host_context: None,
            source_turn_id: None,
            context_remaining: None,
        }
    }

    fn call(action: &str, args: serde_json::Value) -> ToolCall {
        let mut arguments = serde_json::json!({ "action": action });
        if let Some(obj) = args.as_object() {
            for (key, value) in obj {
                arguments[key] = value.clone();
            }
        }
        ToolCall {
            id: "call-1".into(),
            name: "process".into(),
            arguments,
            partial_json: None,
        }
    }

    #[tokio::test]
    async fn start_write_read_stop_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manager = ProcessManager::new();
        let shell = snapshot(temp.path().to_path_buf());

        let started = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call("start", serde_json::json!({ "command": "cat" })),
            &context(),
        )
        .await;
        assert!(started.ok, "got {started:?}");
        let process_id = started
            .value
            .as_ref()
            .and_then(|v| v["processId"].as_str())
            .expect("processId")
            .to_string();

        let written = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call(
                "write",
                serde_json::json!({ "processId": process_id, "data": "hello-process\n" }),
            ),
            &context(),
        )
        .await;
        assert!(written.ok, "got {written:?}");
        assert_eq!(
            written
                .value
                .as_ref()
                .and_then(|v| v["bytesWritten"].as_u64()),
            Some(14)
        );

        let mut echoed = String::new();
        for _ in 0..40 {
            let read = execute_process_tool(
                &manager,
                &policy(),
                &shell,
                false,
                &call("read", serde_json::json!({ "processId": process_id })),
                &context(),
            )
            .await;
            let output = read
                .value
                .as_ref()
                .and_then(|v| v["output"].as_str())
                .unwrap_or_default();
            echoed.push_str(output);
            if echoed.contains("hello-process") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(echoed.contains("hello-process"), "got {echoed:?}");

        let stopped = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call("stop", serde_json::json!({ "processId": process_id })),
            &context(),
        )
        .await;
        assert!(stopped.ok, "got {stopped:?}");

        let listed = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call("list", serde_json::json!({})),
            &context(),
        )
        .await;
        assert_eq!(
            listed
                .value
                .as_ref()
                .and_then(|v| v["processIds"].as_array())
                .map(|a| a.len()),
            Some(0)
        );
    }

    #[tokio::test]
    async fn unknown_process_is_reported() {
        let manager = ProcessManager::new();
        let shell = snapshot(std::env::current_dir().expect("cwd"));
        let result = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call("read", serde_json::json!({ "processId": "proc-999" })),
            &context(),
        )
        .await;
        assert!(!result.ok);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("unknown_process")
        );
    }

    #[tokio::test]
    async fn policy_violation_blocks_start() {
        let manager = ProcessManager::new();
        let shell = snapshot(std::env::current_dir().expect("cwd"));
        let result = execute_process_tool(
            &manager,
            &policy(),
            &shell,
            false,
            &call("start", serde_json::json!({ "command": "sudo rm -rf /" })),
            &context(),
        )
        .await;
        assert!(!result.ok);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("policy_violation")
        );
        assert!(manager.list().is_empty());
    }
}
