// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations and process execution commands are checked against the sandbox policy.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use piko_sandbox::policy::{Access, Policy};

use crate::domain::tools::call::ContentBlock;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutorRef, ToolProviderSource,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

// ---- Provider ----

pub struct WorkspaceToolProvider {
    policy: Arc<Policy>,
    shell_path: String,
}

impl WorkspaceToolProvider {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy: Arc::new(policy),
            shell_path: "bash".into(),
        }
    }

    pub fn with_shell(policy: Policy, shell_path: impl Into<String>) -> Self {
        Self {
            policy: Arc::new(policy),
            shell_path: shell_path.into(),
        }
    }

    /// Create a provider with a permissive policy (read/write current dir).
    pub fn permissive() -> Self {
        Self::new(Policy {
            version: 1,
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from(".")],
            deny: vec![PathBuf::from(".git")],
            allowed_commands: vec![
                "ls".into(),
                "cat".into(),
                "head".into(),
                "tail".into(),
                "find".into(),
                "grep".into(),
                "rg".into(),
                "git".into(),
                "echo".into(),
                "mkdir".into(),
                "cp".into(),
                "mv".into(),
                "rm".into(),
                "wc".into(),
                "sort".into(),
                "uniq".into(),
                "sed".into(),
                "awk".into(),
                "diff".into(),
                "npm".into(),
                "npx".into(),
                "node".into(),
                "bun".into(),
                "cargo".into(),
                "python3".into(),
                "python".into(),
                "go".into(),
                "make".into(),
                "rustc".into(),
                "tsc".into(),
                "biome".into(),
                "prettier".into(),
            ],
            allow_network: false,
        })
    }
}

#[async_trait]
impl ToolProvider for WorkspaceToolProvider {
    fn id(&self) -> &str {
        "workspace"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Workspace
    }

    async fn discover(
        &self,
        _context: ToolDiscoveryContext,
    ) -> Vec<ToolDef> {
        workspace_tools()
    }

    async fn execute(
        &self,
        call: crate::domain::tools::call::ToolCall,
        context: ToolExecutionContext,
    ) -> ToolExecResult {
        execute_workspace_tool(&self.policy, &self.shell_path, &call, &context).await
    }
}

// ---- Tool definitions ----

fn workspace_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "read".into(),
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
            execution_mode: None,
            exposure: None,
            capabilities: Some(vec![ToolCapability::WorkspaceRead]),
            approval: Some(ToolApprovalRequirement::Never),
            metadata: None,
        },
        ToolDef {
            name: "bash".into(),
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

// ---- Tool execution ----

async fn execute_workspace_tool(
    policy: &Policy,
    shell_path: &str,
    call: &ContentBlock,
    _ctx: &ToolExecutionContext,
) -> ToolExecResult {
    let (tool_name, arguments) = match call {
        ContentBlock::ToolCall {
            name, arguments, ..
        } => (name.as_str(), arguments),
        _ => {
            return ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "invalid_call".into(),
                    message: "Not a tool call".into(),
                    retryable: Some(false),
                }),
            };
        }
    };

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
            if let Err(e) = policy.validate_command(command, &cwd) {
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

            // Execute via shell (async)
            let output = if timeout_secs > 0 {
                tokio::process::Command::new("timeout")
                    .arg(format!("{timeout_secs}s"))
                    .arg(shell_path)
                    .arg("-c")
                    .arg(command)
                    .current_dir(&cwd)
                    .output()
                    .await
            } else {
                tokio::process::Command::new(shell_path)
                    .arg("-c")
                    .arg(command)
                    .current_dir(&cwd)
                    .output()
                    .await
            };

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    let combined = format!("{stdout}{stderr}");
                    // Truncate to ~50KB
                    let truncated = if combined.len() > 50_000 {
                        format!(
                            "{}...\n[truncated, full output: {} bytes]",
                            &combined[..50_000],
                            combined.len()
                        )
                    } else {
                        combined
                    };
                    ToolExecResult {
                        ok: out.status.success(),
                        value: Some(serde_json::json!({
                            "output": truncated,
                            "exitCode": out.status.code().unwrap_or(-1),
                        })),
                        error: if out.status.success() {
                            None
                        } else {
                            Some(ToolExecError {
                                code: "command_failed".into(),
                                message: format!("Exit code: {}", out.status.code().unwrap_or(-1)),
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
                                value: Some(
                                    serde_json::json!({"edited": true, "editsApplied": edit_list.len()}),
                                ),
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
