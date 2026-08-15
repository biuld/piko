//! F-34 retry derivation, terminal guidance, and grant visibility.

use crate::domain::tools::approval::ToolApprovalDecision;
use crate::domain::tools::result::ToolExecResult;

const MAX_PATHS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DenialAccess {
    Read,
    Write,
    Unknown,
}

struct DeniedPath {
    path: String,
    access: DenialAccess,
}

pub(super) const NO_GATEWAY_RETRY_NOTE: &str = " The approval-backed retry is unavailable in this session; request with_additional_permissions or require_escalated with a justification, or ask the user.";

const TERMINAL_NEXT_STEP: &str =
    " Prefer a sandboxed alternative or ask the user; do not escalate without explicit consent.";

pub(super) fn denial_retry_args(
    call_args: &serde_json::Value,
    denial_message: &str,
) -> serde_json::Value {
    let mut retry = call_args.clone();
    let denied = denied_paths(denial_message);
    if denied.is_empty() {
        retry["sandbox_permissions"] = serde_json::json!("require_escalated");
        retry["justification"] = serde_json::json!(
            "The enforced sandbox denied the initial command attempt; retry once with explicit elevation"
        );
    } else {
        let write = denied
            .iter()
            .any(|path| matches!(path.access, DenialAccess::Write | DenialAccess::Unknown));
        let read = denied
            .iter()
            .any(|path| matches!(path.access, DenialAccess::Read | DenialAccess::Unknown));
        retry["sandbox_permissions"] = serde_json::json!("with_additional_permissions");
        retry["justification"] = serde_json::json!(if write && !read {
            "The enforced sandbox denied the initial command attempt; retry once with minimal additional write access"
        } else if read && !write {
            "The enforced sandbox denied the initial command attempt; retry once with minimal additional read access"
        } else {
            "The enforced sandbox denied the initial command attempt; retry once with access to the denied path"
        });
        let mut additional = retry
            .get("additional_permissions")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut reads = string_list(&additional, "read_roots");
        let mut writes = string_list(&additional, "write_roots");
        for denied in denied {
            match denied.access {
                DenialAccess::Read => push_unique(&mut reads, denied.path),
                DenialAccess::Write => {
                    let ancestor = nearest_existing_ancestor(&denied.path);
                    push_unique(&mut writes, denied.path);
                    push_unique(&mut writes, ancestor);
                }
                DenialAccess::Unknown => {
                    let ancestor = nearest_existing_ancestor(&denied.path);
                    push_unique(&mut reads, denied.path.clone());
                    push_unique(&mut writes, denied.path);
                    push_unique(&mut writes, ancestor);
                }
            }
        }
        additional["read_roots"] = serde_json::json!(reads);
        additional["write_roots"] = serde_json::json!(writes);
        retry["additional_permissions"] = additional;
    }
    if retry.get("prefix_rule").is_none()
        && let Some(prefix) = reusable_retry_prefix(call_args)
    {
        retry["prefix_rule"] = serde_json::json!(prefix);
    }
    retry
}

pub(super) fn approval_failure(decision: &ToolApprovalDecision) -> (&'static str, String) {
    let (code, message) = match decision {
        ToolApprovalDecision::Expired => (
            "approval_expired",
            "Approval request expired before a decision arrived".into(),
        ),
        ToolApprovalDecision::GuardianDenied { reason } => (
            "guardian_denied",
            format!("Guardian denied approval: {reason}"),
        ),
        ToolApprovalDecision::GuardianUnavailable => (
            "guardian_unavailable",
            "Guardian review failed; failing closed".into(),
        ),
        ToolApprovalDecision::SafetyRejected { reason } => (
            "safety_rejected",
            format!("Write rejected by safety assessment: {reason}"),
        ),
        ToolApprovalDecision::PermissionDenied { reason } => (
            "permission_denied",
            format!("Command denied by permission policy: {reason}"),
        ),
        _ => ("declined", "User declined approval".into()),
    };
    (code, format!("{message}{TERMINAL_NEXT_STEP}"))
}

pub(super) fn attach_approved_grant(
    mut result: ToolExecResult,
    retry_args: &serde_json::Value,
) -> ToolExecResult {
    if !result.ok {
        return result;
    }
    let Some(prefix) = retry_args.get("prefix_rule") else {
        return result;
    };
    if let Some(value) = result.value.as_mut()
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "approved_grant".into(),
            serde_json::json!({
                "prefix": prefix,
                "note": "Commands under this prefix reuse the approved grant for the rest of the session without a new prompt."
            }),
        );
    }
    result
}

fn denied_paths(message: &str) -> Vec<DeniedPath> {
    let mut out = Vec::new();
    for line in message.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("deny") {
            continue;
        }
        let access = if lower.contains("file-write") || lower.contains("deny write") {
            DenialAccess::Write
        } else if lower.contains("file-read") || lower.contains("deny read") {
            DenialAccess::Read
        } else {
            DenialAccess::Unknown
        };
        for token in line.split_whitespace() {
            let token = token.trim_matches(|ch| matches!(ch, '"' | '\'' | '(' | ')' | ','));
            if !token.starts_with('/')
                || token.starts_with("//")
                || token.starts_with("/dev/")
                || out
                    .iter()
                    .any(|existing: &DeniedPath| existing.path == token)
            {
                continue;
            }
            out.push(DeniedPath {
                path: token.to_string(),
                access,
            });
            if out.len() >= MAX_PATHS {
                return out;
            }
        }
    }
    out
}

fn reusable_retry_prefix(call_args: &serde_json::Value) -> Option<Vec<String>> {
    let command = call_args.get("cmd")?.as_str()?;
    if command.is_empty() || command.chars().any(|ch| ";|&<>`$\n\r()".contains(ch)) {
        return None;
    }
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let program = tokens[0];
    let subcommand = tokens[1];
    if !is_reusable_program(program) || !is_subcommand_token(subcommand) {
        return None;
    }
    Some(vec![program.to_string(), subcommand.to_string()])
}

fn is_reusable_program(program: &str) -> bool {
    if program.contains('/') {
        return false;
    }
    !matches!(
        program,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "sudo"
            | "env"
            | "python"
            | "python3"
            | "node"
            | "ruby"
            | "perl"
            | "rm"
            | "curl"
            | "wget"
            | "cd"
            | "export"
            | "echo"
            | "set"
            | "unset"
            | "alias"
            | "source"
            | "exec"
            | "eval"
            | "test"
            | "printf"
            | "read"
            | "true"
            | "false"
            | "time"
            | "local"
            | "shift"
            | "return"
            | "exit"
            | "type"
            | "ulimit"
            | "umask"
            | "wait"
            | "."
    )
}

fn is_subcommand_token(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn string_list(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|existing| existing == &value) {
        items.push(value);
    }
}

fn nearest_existing_ancestor(path: &str) -> String {
    let mut current = std::path::PathBuf::from(path);
    if current.exists() {
        return path.to_string();
    }
    while let Some(parent) = current.parent() {
        if parent.as_os_str().is_empty() {
            break;
        }
        if parent.exists() {
            return parent.display().to_string();
        }
        current = parent.to_path_buf();
    }
    path.to_string()
}
