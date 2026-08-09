use std::path::{Path, PathBuf};
use std::time::Duration;

use piko_sandbox::exec::ExecError;
use piko_sandbox::exec::process::{OutputChunk, TerminationReason};
use piko_sandbox::policy::{Access, EffectivePermissions};

use crate::domain::tools::result::{ToolExecError, ToolExecResult};

use super::SandboxAuthority;

pub(super) fn parse_authority(args: &serde_json::Value) -> Result<SandboxAuthority, &'static str> {
    match args
        .get("sandbox_permissions")
        .and_then(|v| v.as_str())
        .unwrap_or("use_default")
    {
        "use_default" => Ok(SandboxAuthority::Default),
        "with_additional_permissions" => Ok(SandboxAuthority::Additional),
        "require_escalated" => Ok(SandboxAuthority::Escalated),
        _ => Err("invalid sandbox_permissions value"),
    }
}

pub(super) fn containment_policy(
    authority: SandboxAuthority,
    policy: &EffectivePermissions,
    policy_base: &Path,
    workdir: &Path,
    args: &serde_json::Value,
) -> Result<Option<EffectivePermissions>, String> {
    if authority == SandboxAuthority::Escalated {
        return Ok(None);
    }
    let mut effective = policy.clone();
    if authority == SandboxAuthority::Additional {
        let additional = args
            .get("additional_permissions")
            .and_then(|v| v.as_object())
            .ok_or_else(|| "additional_permissions is required".to_string())?;
        effective
            .read_roots
            .extend(additional_roots(additional, "read_roots", policy_base)?);
        let write_roots = additional_roots(additional, "write_roots", policy_base)?;
        validate_write_roots(&mut effective, policy_base, &write_roots)?;
        effective.write_roots.extend(write_roots);
        match additional
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("restricted")
        {
            "enabled" => effective.network = piko_sandbox::policy::NetworkPermissions::Enabled,
            "restricted" => {}
            _ => return Err("additional_permissions.network has an invalid value".into()),
        }
    }
    effective
        .authorize(policy_base, workdir, Access::Read, true)
        .map_err(|error| error.to_string())?;
    Ok(Some(effective))
}

fn validate_write_roots(
    policy: &mut EffectivePermissions,
    policy_base: &Path,
    roots: &[PathBuf],
) -> Result<(), String> {
    for root in roots {
        if root == policy_base || policy_base.starts_with(root) {
            return Err(
                "an additional write root cannot widen authority to the whole workspace".into(),
            );
        }
        for denied in &policy.denied_read_roots {
            let denied = resolve_policy_root(denied, policy_base);
            if root.starts_with(&denied) || denied.starts_with(root) {
                return Err(format!(
                    "additional write root overlaps a non-overridable deny path: {}",
                    root.display()
                ));
            }
        }
        policy
            .denied_write_roots
            .retain(|denied| resolve_policy_root(denied, policy_base) != *root);
    }
    Ok(())
}

fn additional_roots(
    permissions: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    cwd: &Path,
) -> Result<Vec<PathBuf>, String> {
    let Some(values) = permissions.get(key) else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| format!("additional_permissions.{key} must be an array"))?;
    values
        .iter()
        .map(|value| {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("additional_permissions.{key} entries must be strings"))?;
            let path = Path::new(raw);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            Ok(absolute.canonicalize().unwrap_or(absolute))
        })
        .collect()
}

fn resolve_policy_root(root: &Path, cwd: &Path) -> PathBuf {
    let path = if root.is_absolute() {
        root.to_path_buf()
    } else {
        cwd.join(root)
    };
    path.canonicalize().unwrap_or(path)
}

pub(super) fn materialize_policy(
    policy: &EffectivePermissions,
    base: &Path,
) -> EffectivePermissions {
    let roots = |items: &[PathBuf]| {
        items
            .iter()
            .map(|root| resolve_policy_root(root, base))
            .collect()
    };
    EffectivePermissions {
        version: policy.version,
        read_roots: roots(&policy.read_roots),
        write_roots: roots(&policy.write_roots),
        scratch_roots: roots(&policy.scratch_roots),
        denied_read_roots: roots(&policy.denied_read_roots),
        denied_write_roots: roots(&policy.denied_write_roots),
        network: policy.network,
    }
}

pub(super) fn observation(
    session_id: &str,
    chunk: OutputChunk,
    original_bytes: usize,
    elapsed: Duration,
) -> ToolExecResult {
    let state = match chunk.termination {
        Some(TerminationReason::TimedOut) => "timed_out",
        Some(TerminationReason::Cancelled) => "cancelled",
        Some(TerminationReason::Terminated) => "terminated",
        None if !chunk.exited => "running",
        None if chunk.status.and_then(|s| s.signal).is_some() => "signalled",
        None => "exited",
    };
    let output = String::from_utf8_lossy(&chunk.bytes).to_string();
    let mut value = serde_json::json!({
        "state": state,
        "output": output,
        "truncated": chunk.truncated,
        "original_token_count": approximate_tokens(original_bytes),
        "wall_time_seconds": elapsed.as_secs_f64(),
    });
    if !chunk.exited {
        value["session_id"] = serde_json::json!(session_id);
    } else if let Some(exit_code) = chunk.status.and_then(|status| status.code) {
        value["exit_code"] = serde_json::json!(exit_code);
    } else if let Some(signal) = chunk.status.and_then(|status| status.signal) {
        value["signal"] = serde_json::json!(signal);
    }
    ToolExecResult {
        ok: true,
        value: Some(value),
        error: None,
    }
}

pub(super) fn sandbox_observation_error(
    sandboxed: bool,
    chunk: &OutputChunk,
) -> Option<ToolExecResult> {
    if !sandboxed || !chunk.exited || chunk.status.is_some_and(|status| status.code == Some(0)) {
        return None;
    }
    let output = String::from_utf8_lossy(&chunk.bytes);
    let normalized = output.to_ascii_lowercase();
    if normalized.contains("sandbox-exec:")
        && (normalized.contains("deny") || normalized.contains("operation not permitted"))
    {
        return Some(error_result("sandbox_denied", output.into_owned(), true));
    }
    if normalized.starts_with("bwrap:") {
        return Some(error_result(
            "sandbox_unavailable",
            output.into_owned(),
            false,
        ));
    }
    None
}

pub(super) fn tokens_to_bytes(tokens: u64) -> usize {
    tokens.saturating_mul(4).min(16 * 1024 * 1024) as usize
}

fn approximate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

pub(super) fn invalid_args(message: impl Into<String>) -> ToolExecResult {
    error_result("invalid_args", message.into(), false)
}

pub(super) fn policy_error(message: String) -> ToolExecResult {
    error_result("sandbox_denied", message, true)
}

pub(super) fn exec_error(error: ExecError) -> ToolExecResult {
    match error {
        ExecError::SandboxUnavailable(message) => {
            error_result("sandbox_unavailable", message, false)
        }
        ExecError::EffectivePermissions(error) => {
            error_result("sandbox_setup_failed", error.to_string(), false)
        }
        ExecError::Spawn(error) => error_result("spawn_failed", error.to_string(), false),
        other => error_result("spawn_failed", other.to_string(), false),
    }
}

pub(super) fn io_error(message: String) -> ToolExecResult {
    error_result("io_error", message, false)
}

fn error_result(code: &str, message: String, retryable: bool) -> ToolExecResult {
    ToolExecResult {
        ok: false,
        value: None,
        error: Some(ToolExecError {
            code: code.into(),
            message,
            retryable: Some(retryable),
        }),
    }
}
