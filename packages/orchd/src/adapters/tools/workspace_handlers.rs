// ---- Workspace file tools and the aggregate tool catalog ----
//
// This file owns the file tools (read/edit/write), the read-only
// `environment` tool, and the aggregate catalog consumed by
// `WorkspaceToolProvider::discover`. The `bash` tool lives in
// `shell_handlers.rs` and the long-lived `process` tool in
// `process_handlers.rs` (F-08 slice 2).

use std::path::Path;

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
    cwd: &Path,
    policy: &Policy,
    call: &ToolCall,
    _ctx: &ToolExecutionContext,
) -> ToolExecResult {
    let tool_name = call.name.as_str();
    let arguments = &call.arguments;

    match tool_name {
        "read" => {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let offset = arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let limit = arguments.get("limit").and_then(|v| v.as_u64());

            match policy.authorize(cwd, Path::new(path), Access::Read, true) {
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

            match policy.authorize(cwd, Path::new(path), Access::Write, true) {
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
                            if old.is_empty() {
                                return ToolExecResult {
                                    ok: false,
                                    value: None,
                                    error: Some(ToolExecError {
                                        code: "edit_requires_old_text".into(),
                                        message:
                                            "oldText must not be empty; include the exact text to replace"
                                                .into(),
                                        retryable: Some(false),
                                    }),
                                };
                            }
                            let positions: Vec<usize> =
                                modified.match_indices(old).map(|(pos, _)| pos).collect();
                            match positions.len() {
                                1 => {
                                    let start = positions[0];
                                    modified.replace_range(start..start + old.len(), new);
                                }
                                0 => {
                                    return ToolExecResult {
                                        ok: false,
                                        value: None,
                                        error: Some(ToolExecError {
                                            code: "edit_not_found".into(),
                                            message: format!(
                                                "oldText not found in file: '{}'. Read the file and provide the exact text with more surrounding context.",
                                                &old[..old.len().min(80)]
                                            ),
                                            retryable: Some(false),
                                        }),
                                    };
                                }
                                n => {
                                    let lines: Vec<String> = positions
                                        .iter()
                                        .take(3)
                                        .map(|pos| {
                                            (modified[..*pos].matches('\n').count() + 1).to_string()
                                        })
                                        .collect();
                                    return ToolExecResult {
                                        ok: false,
                                        value: None,
                                        error: Some(ToolExecError {
                                            code: "edit_not_unique".into(),
                                            message: format!(
                                                "oldText matches {n} times in file (at line{} {}); add more surrounding context so the match is unique, or split it into separate edits.",
                                                if n > 1 { "s" } else { "" },
                                                lines.join(", ")
                                            ),
                                            retryable: Some(false),
                                        }),
                                    };
                                }
                            }
                        }
                        if let Err(e) = policy.verify_resolved(
                            cwd,
                            Path::new(path),
                            Access::Write,
                            true,
                            &resolved,
                        ) {
                            return ToolExecResult {
                                ok: false,
                                value: None,
                                error: Some(ToolExecError {
                                    code: "access_denied".into(),
                                    message: e.to_string(),
                                    retryable: Some(false),
                                }),
                            };
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

            match policy.authorize(cwd, Path::new(path), Access::Write, false) {
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
                    if let Err(e) = policy.verify_resolved(
                        cwd,
                        Path::new(path),
                        Access::Write,
                        false,
                        &resolved,
                    ) {
                        return ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "access_denied".into(),
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
    use crate::domain::tools::call::ToolCall;
    use piko_orchd_api::tools::ToolExecutionContext;
    use std::path::PathBuf;

    fn policy() -> Policy {
        Policy {
            version: 1,
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from(".")],
            deny: vec![PathBuf::from(".git"), PathBuf::from(".piko")],
            allowed_commands: vec![],
            allow_network: false,
        }
    }

    fn context() -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session".into(),
            agent_instance_id: "agent".into(),
            execution_id: "exec".into(),
            cancellation: None,
            agent_id: "root".into(),
            agent_role: None,
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

    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: args,
            partial_json: None,
        }
    }

    #[tokio::test]
    async fn edit_applies_unique_replacement() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn one() {}\nfn two() {}\n").unwrap();

        let result = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "edit",
                serde_json::json!({
                    "path": "a.rs",
                    "edits": [{ "oldText": "fn one() {}", "newText": "fn renamed() {}" }]
                }),
            ),
            &context(),
        )
        .await;
        assert!(result.ok, "{:?}", result.error);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("a.rs")).unwrap(),
            "fn renamed() {}\nfn two() {}\n"
        );
    }

    #[tokio::test]
    async fn edit_rejects_empty_old_text() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn one() {}\n").unwrap();

        let result = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "edit",
                serde_json::json!({
                    "path": "a.rs",
                    "edits": [{ "oldText": "", "newText": "x" }]
                }),
            ),
            &context(),
        )
        .await;
        let error = result.error.expect("empty oldText must fail");
        assert_eq!(error.code, "edit_requires_old_text");
        assert!(!result.ok);
    }

    #[tokio::test]
    async fn edit_rejects_non_unique_match_with_line_numbers() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("a.rs"),
            "let x = 1;\nlet y = 2;\nlet x = 3;\n",
        )
        .unwrap();

        let result = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "edit",
                serde_json::json!({
                    "path": "a.rs",
                    "edits": [{ "oldText": "let x =", "newText": "let z =" }]
                }),
            ),
            &context(),
        )
        .await;
        let error = result.error.expect("non-unique match must fail");
        assert_eq!(error.code, "edit_not_unique");
        assert!(error.message.contains("2 times"));
        assert!(error.message.contains("at lines 1, 3"));
        assert!(!result.ok);
    }

    #[tokio::test]
    async fn edit_not_found_message_guides_the_model() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn one() {}\n").unwrap();

        let result = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "edit",
                serde_json::json!({
                    "path": "a.rs",
                    "edits": [{ "oldText": "fn missing() {}", "newText": "fn x() {}" }]
                }),
            ),
            &context(),
        )
        .await;
        let error = result.error.expect("missing oldText must fail");
        assert_eq!(error.code, "edit_not_found");
        assert!(error.message.contains("Read the file"));
        assert!(!result.ok);
    }

    #[tokio::test]
    async fn write_and_edit_are_denied_inside_dot_piko() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".piko")).unwrap();
        std::fs::write(temp.path().join(".piko/approvals.json"), "{}").unwrap();

        let write = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "write",
                serde_json::json!({
                    "path": ".piko/approvals.json",
                    "content": r#"{"fingerprints":{"bash:git":{"tool_name":"bash"}}}"#
                }),
            ),
            &context(),
        )
        .await;
        let error = write.error.expect(".piko write must be denied");
        assert_eq!(error.code, "access_denied");
        assert!(!write.ok);
        assert_eq!(
            std::fs::read_to_string(temp.path().join(".piko/approvals.json")).unwrap(),
            "{}",
            "approvals file must remain untouched"
        );

        let edit = execute_workspace_tool(
            temp.path(),
            &policy(),
            &call(
                "edit",
                serde_json::json!({
                    "path": ".piko/approvals.json",
                    "edits": [{ "oldText": "{}", "newText": "{\"granted\":true}" }]
                }),
            ),
            &context(),
        )
        .await;
        let error = edit.error.expect(".piko edit must be denied");
        assert_eq!(error.code, "access_denied");
        assert!(!edit.ok);
    }
}
