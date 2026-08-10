//! Unified command execution tools.
//!
//! `exec_command` always accepts a complete shell program. Authorization
//! selects the containment policy; it never attempts to validate shell
//! syntax. Commands that outlive the initial yield remain addressable by
//! `write_stdin` through the provider-owned process manager.

use std::path::Path;
use std::time::{Duration, Instant};

use piko_sandbox::exec::process::{DEFAULT_MAX_OUTPUT_BYTES, ProcessManager};
use piko_sandbox::exec::{ShellSnapshot, SpawnConfig};
use piko_sandbox::policy::EffectivePermissions;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutorRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;

const DEFAULT_YIELD_MS: u64 = 30_000;
const DEFAULT_POLL_MS: u64 = 5_000;
const MAX_YIELD_MS: u64 = 30_000;
const STOP_GRACE: Duration = Duration::from_secs(2);

mod support;
use support::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SandboxAuthority {
    Default,
    Additional,
    Escalated,
}

pub(super) fn exec_command_tool_def() -> ToolDef {
    ToolDef {
        name: "exec_command".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/exec-command"),
        description: "Run a complete shell command in a PTY. Non-zero exit codes are normal command results. Commands still running after the initial yield (default 30s) return a running result with a session_id; poll it with write_stdin until the result reports exited. Prefer `rg` over `find` for searches; when using `find`, prune heavy directories (e.g. `-not -path '*/target/*' -not -path '*/.git/*'`).".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string", "description": "Complete shell program" },
                "workdir": { "type": "string", "description": "Working directory; relative paths resolve from the session cwd" },
                "tty": { "type": "boolean", "description": "Allocate a PTY for interactive or terminal-sensitive commands; defaults to false" },
                "yield_time_ms": { "type": "integer", "minimum": 0, "maximum": MAX_YIELD_MS, "description": "Initial wait before returning a running result; defaults to 30000" },
                "timeout_ms": { "type": "integer", "minimum": 1 },
                "max_output_tokens": { "type": "integer", "minimum": 1 },
                "sandbox_permissions": {
                    "type": "string",
                    "enum": ["use_default", "with_additional_permissions", "require_escalated"]
                },
                "additional_permissions": {
                    "type": "object",
                    "properties": {
                        "read_roots": { "type": "array", "items": { "type": "string" } },
                        "write_roots": { "type": "array", "items": { "type": "string" } },
                        "network": { "type": "string", "enum": ["restricted", "enabled"] }
                    }
                },
                "justification": { "type": "string" },
                "prefix_rule": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["cmd"]
        }),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: "exec_command".into(),
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

pub(super) fn write_stdin_tool_def() -> ToolDef {
    ToolDef {
        name: "write_stdin".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", "workspace/write-stdin"),
        description: "Write to, poll, or terminate a running exec_command session.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "chars": { "type": "string" },
                "terminate": { "type": "boolean" },
                "yield_time_ms": { "type": "integer", "minimum": 0, "maximum": MAX_YIELD_MS },
                "max_output_tokens": { "type": "integer", "minimum": 1 }
            },
            "required": ["session_id"]
        }),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: "write_stdin".into(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: Some(vec![ToolCapability::Process]),
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

pub(super) async fn execute_exec_command(
    processes: &ProcessManager,
    policy: &EffectivePermissions,
    shell: &ShellSnapshot,
    call: &ToolCall,
    ctx: &ToolExecutionContext,
) -> ToolExecResult {
    let args = &call.arguments;
    let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
    if cmd.trim().is_empty() {
        return invalid_args("cmd must be a non-empty string");
    }
    let authority = match parse_authority(args) {
        Ok(authority) => authority,
        Err(message) => return invalid_args(message),
    };
    if authority != SandboxAuthority::Default
        && args
            .get("justification")
            .and_then(|v| v.as_str())
            .is_none_or(|value| value.trim().is_empty())
    {
        return invalid_args("justification is required when requesting extra authority");
    }

    let mut snapshot = shell.clone();
    let policy_base = snapshot
        .cwd
        .canonicalize()
        .unwrap_or_else(|_| snapshot.cwd.clone());
    let effective_policy = materialize_policy(policy, &policy_base);
    if let Some(workdir) = args.get("workdir").and_then(|v| v.as_str()) {
        let requested = Path::new(workdir);
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            snapshot.cwd.join(requested)
        };
        match candidate.canonicalize() {
            Ok(path) if path.is_dir() => snapshot.cwd = path,
            Ok(_) => return invalid_args("workdir is not a directory"),
            Err(error) => return invalid_args(format!("invalid workdir: {error}")),
        }
    }

    let containment = match containment_policy(
        authority,
        &effective_policy,
        &policy_base,
        &snapshot.cwd,
        args,
    ) {
        Ok(policy) => policy,
        Err(error) => return policy_error(error),
    };
    let max_output_bytes = args
        .get("max_output_tokens")
        .and_then(|v| v.as_u64())
        .map(tokens_to_bytes)
        .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    let config = SpawnConfig {
        command: cmd.to_string(),
        shell: snapshot,
        tty: args.get("tty").and_then(|v| v.as_bool()).unwrap_or(false),
        policy: containment,
        timeout: args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(Duration::from_millis),
        cancel: ctx.cancellation.clone(),
        kill_grace: STOP_GRACE,
        max_output_bytes,
    };

    let started = Instant::now();
    let process = match processes.start(config).await {
        Ok(process) => process,
        Err(error) => return exec_error(error),
    };
    let yield_ms = args
        .get("yield_time_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_YIELD_MS)
        .min(MAX_YIELD_MS);
    if yield_ms > 0 {
        let _ = process.wait_for_exit(Duration::from_millis(yield_ms)).await;
    }
    if process.exited() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let chunk = process.try_read_output();
    if let Some(error) = sandbox_observation_error(process.sandboxed(), &chunk) {
        processes.remove(process.id());
        return error;
    }
    let original_bytes = chunk.bytes.len();
    let result = observation(process.id(), chunk, original_bytes, started.elapsed());
    if process.exited() {
        processes.remove(process.id());
    }
    result
}

pub(super) async fn execute_write_stdin(
    processes: &ProcessManager,
    call: &ToolCall,
) -> ToolExecResult {
    let args = &call.arguments;
    let Some(session_id) = args.get("session_id").and_then(|v| v.as_str()) else {
        return invalid_args("session_id is required");
    };
    let Some(process) = processes.get(session_id) else {
        return ToolExecResult {
            ok: false,
            value: None,
            error: Some(ToolExecError {
                code: "unknown_session".into(),
                message: format!("unknown or completed exec session: {session_id}"),
                retryable: Some(false),
            }),
        };
    };
    let started = Instant::now();
    if let Some(chars) = args.get("chars").and_then(|v| v.as_str())
        && let Err(error) = process.write_stdin(chars.as_bytes()).await
    {
        return io_error(format!("write_stdin failed: {error}"));
    }
    if args
        .get("terminate")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        process.stop(STOP_GRACE).await;
    } else {
        let default_yield = if args.get("chars").is_some() {
            250
        } else {
            DEFAULT_POLL_MS
        };
        let yield_ms = args
            .get("yield_time_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(default_yield)
            .min(MAX_YIELD_MS);
        if yield_ms > 0 {
            let _ = process.wait_for_exit(Duration::from_millis(yield_ms)).await;
        }
    }
    if process.exited() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut chunk = process.try_read_output();
    if let Some(error) = sandbox_observation_error(process.sandboxed(), &chunk) {
        processes.remove(session_id);
        return error;
    }
    let original_bytes = chunk.bytes.len();
    if let Some(tokens) = args.get("max_output_tokens").and_then(|v| v.as_u64()) {
        let limit = tokens_to_bytes(tokens);
        if chunk.bytes.len() > limit {
            chunk.bytes.truncate(limit);
            chunk.truncated = true;
        }
    }
    let result = observation(session_id, chunk, original_bytes, started.elapsed());
    if process.exited() {
        processes.remove(session_id);
    }
    result
}
