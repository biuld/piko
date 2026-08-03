// ---- Workspace file tools and the aggregate tool catalog ----
//
// This file owns the file tools (read/edit/write), the read-only
// `environment` tool, and the aggregate catalog consumed by
// `WorkspaceToolProvider::discover`. The `bash` tool lives in
// `shell_handlers.rs` and the long-lived `process` tool in
// `process_handlers.rs` (F-08 slice 2).

use std::path::{Path, PathBuf};

use piko_sandbox::policy::{Access, Policy};

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;

use super::process_handlers::process_tool_def;
use super::shell_handlers::shell_tool_def;

pub(super) fn workspace_tools() -> Vec<ToolDef> {
    vec![
        read_tool_def(),
        shell_tool_def(),
        edit_tool_def(),
        write_tool_def(),
        process_tool_def(),
        environment_tool_def(),
    ]
}

fn read_tool_def() -> ToolDef {
    ToolDef {
        name: "read".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/read"),
        description:
            "Read file contents. Supports text files. Output truncated to 2000 lines / 50KB.".into(),
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
    }
}

fn edit_tool_def() -> ToolDef {
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
            target: "edit".into(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: Some(vec![ToolCapability::WorkspaceWrite]),
        approval: Some(ToolApprovalRequirement::OnRequest),
        metadata: None,
    }
}

fn write_tool_def() -> ToolDef {
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
    }
}

fn environment_tool_def() -> ToolDef {
    ToolDef {
        name: "environment".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/environment"),
        description:
            "Report the execution environment: resolved shell, OS, architecture, cwd, PATH, and detected tools."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: "environment".into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Parallel),
        exposure: None,
        capabilities: Some(vec![ToolCapability::WorkspaceRead]),
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

/// Execute the file tools (read/edit/write). The provider routes `bash`,
/// `process`, and `environment` to their dedicated handlers.
pub(super) async fn execute_workspace_tool(
    policy: &Policy,
    call: &ToolCall,
    _ctx: &ToolExecutionContext,
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
