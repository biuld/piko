// ---- Workspace tool definitions and handlers ----
//
// Built-in workspace tools and their sandbox-policy-checked implementations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use piko_sandbox::exec::{ShellSnapshot, SpawnConfig};
use piko_sandbox::policy::{Access, Policy};

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;

pub(super) fn workspace_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "read".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/read"),
            description:
                "Read file contents. Supports text files. Output truncated to 2000 lines / 50KB."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file (relative or absolute)" },
                    "offset": { "type": "number", "description": "Line to start from (1-indexed)" },
                    "limit": { "type": "number", "description": "Max lines to read" }
                },
                "required": ["path"]
            }),
            executor: ToolExecutorRef {
                kind: "native".into(),
                target: "read".into(),
                extra: None,
            },
            execution_mode: Some(ToolExecutionMode::Parallel),
            exposure: None,
            capabilities: Some(vec![ToolCapability::WorkspaceRead]),
            approval: Some(ToolApprovalRequirement::Never),
            metadata: None,
        },
        ToolDef {
            name: "bash".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/bash"),
            description: "Execute a bash command. Output truncated to 2000 lines / 50KB.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Bash command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds" }
                },
                "required": ["command"]
            }),
            executor: ToolExecutorRef {
                kind: "native".into(),
                target: "shell".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: Some(vec![ToolCapability::Process, ToolCapability::WorkspaceRead]),
            approval: Some(ToolApprovalRequirement::OnRequest),
            metadata: None,
        },
        ToolDef {
            name: "edit".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/edit"),
            description: "Edit a file with exact text replacement.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "oldText": { "type": "string" },
                                "newText": { "type": "string" }
                            },
                            "required": ["oldText", "newText"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
            executor: ToolExecutorRef {
                kind: "native".into(),
                target: "apply_patch".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: Some(vec![ToolCapability::WorkspaceWrite]),
            approval: Some(ToolApprovalRequirement::OnRequest),
            metadata: None,
        },
        ToolDef {
            name: "write".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/write"),
            description: "Write content to a file. Creates parent directories.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            executor: ToolExecutorRef {
                kind: "native".into(),
                target: "write".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: Some(vec![ToolCapability::WorkspaceWrite]),
            approval: Some(ToolApprovalRequirement::OnRequest),
            metadata: None,
        },
    ]
}

pub(super) async fn execute_workspace_tool(
    policy: &Policy,
    shell: &ShellSnapshot,
    os_sandbox: bool,
    call: &ToolCall,
    ctx: &ToolExecutionContext,
) -> ToolExecResult {
    let tool_name = call.name.as_str();
    let arguments = &call.arguments;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match tool_name {
        "read" => {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let offset = arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let limit = arguments.get("limit").and_then(|v| v.as_u64());

            match policy.authorize(&cwd, Path::new(path), Access::Read, true) {
                Ok(resolved) => match tokio::fs::read_to_string(&resolved).await {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let total = lines.len();
                        let start = offset.saturating_sub(1).min(total);
                        let end = limit
                            .map(|l| (start + l as usize).min(total))
                            .unwrap_or(total);
                        let selected = &lines[start..end];
                        let text = selected.join("\n");
                        ToolExecResult {
                            ok: true,
                            value: Some(serde_json::json!({
                                "content": text,
                                "totalLines": total,
                                "linesRead": selected.len(),
                            })),
                            error: None,
                        }
                    }
                    Err(e) => ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "io_error".into(),
                            message: format!("Failed to read {}: {e}", resolved.display()),
                            retryable: Some(false),
                        }),
                    },
                },
                Err(e) => ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "access_denied".into(),
                        message: e.to_string(),
                        retryable: Some(false),
                    }),
                },
            }
        }
        "bash" => {
            let command = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let timeout_secs = arguments
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // Validate command against policy
            if let Err(e) = policy.validate_command(command, &shell.cwd) {
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

            // Execute through the piko-sandbox PTY runner: process-group
            // lifecycle, timeout, cancellation grace, and the platform OS
            // sandbox when enabled (F-08).
            let spawn = SpawnConfig {
                command: command.to_string(),
                shell: shell.clone(),
                policy: os_sandbox.then(|| policy.clone()),
                timeout: (timeout_secs > 0).then(|| Duration::from_secs(timeout_secs)),
                cancel: ctx.cancellation.clone(),
                kill_grace: Duration::from_secs(2),
                max_output_bytes: 65_536,
            };
            match piko_sandbox::exec::run(spawn).await {
                Ok(outcome) => {
                    let combined = outcome.output;
                    // Truncate to ~50KB
                    let truncated = if combined.len() > 50_000 {
                        let mut end = 50_000;
                        while end > 0 && !combined.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!(
                            "{}...\n[truncated, full output: {} bytes]",
                            &combined[..end],
                            combined.len()
                        )
                    } else {
                        combined
                    };
                    let exit_code = outcome.status.code.unwrap_or(-1);
                    let success =
                        !outcome.timed_out && !outcome.cancelled && outcome.status.code == Some(0);
                    ToolExecResult {
                        ok: success,
                        value: Some(serde_json::json!({
                            "output": truncated,
                            "exitCode": exit_code,
                            "signal": outcome.status.signal,
                            "timedOut": outcome.timed_out,
                            "cancelled": outcome.cancelled,
                        })),
                        error: if success {
                            None
                        } else if outcome.timed_out {
                            Some(ToolExecError {
                                code: "timed_out".into(),
                                message: "Command timed out and was terminated".into(),
                                retryable: Some(false),
                            })
                        } else if outcome.cancelled {
                            Some(ToolExecError {
                                code: "cancelled".into(),
                                message: "Command cancelled".into(),
                                retryable: Some(false),
                            })
                        } else if let Some(signal) = outcome.status.signal {
                            Some(ToolExecError {
                                code: "command_signalled".into(),
                                message: format!("Command terminated by signal {signal}"),
                                retryable: Some(false),
                            })
                        } else {
                            Some(ToolExecError {
                                code: "command_failed".into(),
                                message: format!("Exit code: {exit_code}"),
                                retryable: Some(false),
                            })
                        },
                    }
                }
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
        "edit" => {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let edits = arguments.get("edits").and_then(|v| v.as_array());

            match policy.authorize(&cwd, Path::new(path), Access::Write, true) {
                Ok(resolved) => {
                    let content = match tokio::fs::read_to_string(&resolved).await {
                        Ok(c) => c,
                        Err(e) => {
                            return ToolExecResult {
                                ok: false,
                                value: None,
                                error: Some(ToolExecError {
                                    code: "io_error".into(),
                                    message: format!("Failed to read {}: {e}", resolved.display()),
                                    retryable: Some(false),
                                }),
                            };
                        }
                    };

                    if let Some(edit_list) = edits {
                        let mut modified = content.clone();
                        for edit in edit_list {
                            let old = edit.get("oldText").and_then(|v| v.as_str()).unwrap_or("");
                            let new = edit.get("newText").and_then(|v| v.as_str()).unwrap_or("");
                            if let Some(pos) = modified.find(old) {
                                let start = pos;
                                let end = pos + old.len();
                                modified.replace_range(start..end, new);
                            } else {
                                return ToolExecResult {
                                    ok: false,
                                    value: None,
                                    error: Some(ToolExecError {
                                        code: "edit_not_found".into(),
                                        message: format!(
                                            "oldText not found in file: '{}'",
                                            &old[..old.len().min(80)]
                                        ),
                                        retryable: Some(false),
                                    }),
                                };
                            }
                        }
                        match tokio::fs::write(&resolved, &modified).await {
                            Ok(_) => ToolExecResult {
                                ok: true,
                                value: Some(serde_json::json!({
                                    "edited": true,
                                    "editsApplied": edit_list.len()
                                })),
                                error: None,
                            },
                            Err(e) => ToolExecResult {
                                ok: false,
                                value: None,
                                error: Some(ToolExecError {
                                    code: "io_error".into(),
                                    message: e.to_string(),
                                    retryable: Some(false),
                                }),
                            },
                        }
                    } else {
                        ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "invalid_args".into(),
                                message: "edits must be an array".into(),
                                retryable: Some(false),
                            }),
                        }
                    }
                }
                Err(e) => ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "access_denied".into(),
                        message: e.to_string(),
                        retryable: Some(false),
                    }),
                },
            }
        }
        "write" => {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = arguments
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match policy.authorize(&cwd, Path::new(path), Access::Write, false) {
                Ok(resolved) => {
                    if let Some(parent) = resolved.parent()
                        && let Err(e) = tokio::fs::create_dir_all(parent).await
                    {
                        return ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "io_error".into(),
                                message: e.to_string(),
                                retryable: Some(false),
                            }),
                        };
                    }
                    match tokio::fs::write(&resolved, content).await {
                        Ok(_) => ToolExecResult {
                            ok: true,
                            value: Some(serde_json::json!({"written": true})),
                            error: None,
                        },
                        Err(e) => ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "io_error".into(),
                                message: e.to_string(),
                                retryable: Some(false),
                            }),
                        },
                    }
                }
                Err(e) => ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "access_denied".into(),
                        message: e.to_string(),
                        retryable: Some(false),
                    }),
                },
            }
        }
        _ => ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "unknown_tool".into(),
                message: format!("Unknown workspace tool: {tool_name}"),
                retryable: Some(false),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_orchd_api::tools::ToolExecutionContext;
    use piko_protocol::messages::ToolCall;

    fn test_policy() -> Policy {
        Policy {
            version: 1,
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from(".")],
            deny: vec![],
            allowed_commands: vec![
                "exit".into(),
                "echo".into(),
                "kill".into(),
                "sleep".into(),
                "pwd".into(),
                "seq".into(),
                "tr".into(),
            ],
            allow_network: false,
        }
    }

    fn context(cancel: Option<tokio_util::sync::CancellationToken>) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session".into(),
            agent_instance_id: "agent".into(),
            execution_id: "exec".into(),
            cancellation: cancel,
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

    fn bash_call(command: &str, timeout_secs: Option<u64>) -> ToolCall {
        let mut arguments = serde_json::json!({ "command": command });
        if let Some(secs) = timeout_secs {
            arguments["timeout"] = serde_json::json!(secs);
        }
        ToolCall {
            id: "call-1".into(),
            name: "bash".into(),
            arguments,
            partial_json: None,
        }
    }

    fn snapshot(cwd: std::path::PathBuf) -> ShellSnapshot {
        ShellSnapshot {
            shell_path: "bash".into(),
            cwd,
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        }
    }

    #[tokio::test]
    async fn bash_reports_exit_code_and_signal() {
        let cwd = std::env::current_dir().expect("cwd");
        let policy = test_policy();
        let shell = snapshot(cwd.clone());

        let exit = execute_workspace_tool(
            &policy,
            &shell,
            false,
            &bash_call("exit 42", None),
            &context(None),
        )
        .await;
        assert!(!exit.ok);
        assert_eq!(
            exit.value.as_ref().and_then(|v| v["exitCode"].as_i64()),
            Some(42)
        );

        let signalled = execute_workspace_tool(
            &policy,
            &shell,
            false,
            &bash_call("kill -KILL $$", None),
            &context(None),
        )
        .await;
        assert!(!signalled.ok);
        assert_eq!(
            signalled.value.as_ref().and_then(|v| v["signal"].as_i64()),
            Some(9)
        );
        assert_eq!(
            signalled.error.as_ref().map(|e| e.code.as_str()),
            Some("command_signalled")
        );
    }

    #[tokio::test]
    async fn bash_timeout_is_bounded_and_reported() {
        let cwd = std::env::current_dir().expect("cwd");
        let policy = test_policy();
        let shell = snapshot(cwd);
        let result = execute_workspace_tool(
            &policy,
            &shell,
            false,
            &bash_call("sleep 30", Some(1)),
            &context(None),
        )
        .await;
        assert!(!result.ok);
        assert_eq!(
            result.value.as_ref().and_then(|v| v["timedOut"].as_bool()),
            Some(true)
        );
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("timed_out")
        );
    }

    #[tokio::test]
    async fn bash_respects_runtime_cancellation() {
        let cwd = std::env::current_dir().expect("cwd");
        let policy = test_policy();
        let shell = snapshot(cwd);
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let result = execute_workspace_tool(
            &policy,
            &shell,
            false,
            &bash_call("sleep 30", None),
            &context(Some(token)),
        )
        .await;
        assert!(!result.ok);
        assert_eq!(
            result.value.as_ref().and_then(|v| v["cancelled"].as_bool()),
            Some(true)
        );
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("cancelled")
        );
    }

    #[tokio::test]
    async fn bash_uses_shell_snapshot_cwd_and_env() {
        let temp = tempfile::tempdir().expect("tempdir");
        let policy = test_policy();
        let shell = ShellSnapshot {
            shell_path: "bash".into(),
            cwd: temp.path().to_path_buf(),
            env: vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("PIKO_TOOL_VAR".into(), "tool-env-ok".into()),
            ],
        };
        let result = execute_workspace_tool(
            &policy,
            &shell,
            false,
            &bash_call("pwd; echo $PIKO_TOOL_VAR", None),
            &context(None),
        )
        .await;
        assert!(result.ok, "got {result:?}");
        let output = result
            .value
            .as_ref()
            .and_then(|v| v["output"].as_str())
            .unwrap_or_default();
        assert!(output.contains(&temp.path().display().to_string()));
        assert!(output.contains("tool-env-ok"));
    }
}
