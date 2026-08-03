//! F-12 write safety assessment: deterministic, host-owned gate for
//! workspace write approvals.
//!
//! Before the guardian or user flow, the approval gateway asks whether a
//! write target is fully inside the sandbox's writable roots. A fully
//! constrained write is auto-approved one-shot (the policy enforces the
//! boundary at execution); an out-of-roots write fails closed because
//! execution would deny it regardless of approval. Requests that cannot be
//! assessed fall through to the existing flow unchanged.

use std::path::{Component, Path, PathBuf};

/// Outcome of assessing a write request against the sandbox writable roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteSafetyDecision {
    /// Every target is fully inside a writable root; the write can execute
    /// one-shot without a prompt (the policy enforces the boundary).
    AutoApprove,
    /// The request cannot be assessed (non-write tool, missing path, or no
    /// roots); the existing user/guardian flow owns the decision.
    AskUser,
    /// A target lies outside every writable root; execution would deny it
    /// regardless of approval.
    Reject { reason: String },
}

/// Workspace write tools covered by the deterministic safety gate.
pub fn is_write_tool(tool_name: &str) -> bool {
    matches!(tool_name, "edit" | "write")
}

/// Resolved F-12 safety behavior for the approval gateway.
#[derive(Debug, Clone)]
pub struct SafetyConfig {
    pub auto_approve_workspace_writes: bool,
}

impl SafetyConfig {
    pub fn from_settings(settings: Option<&crate::domain::config::SafetySettings>) -> Self {
        Self {
            auto_approve_workspace_writes: settings
                .and_then(|settings| settings.auto_approve_workspace_writes)
                .unwrap_or(true),
        }
    }
}

/// Assess a write request against the provider's writable roots.
///
/// `writable_roots` are absolute paths projected by the workspace provider;
/// `cwd` is the session working directory used to resolve relative targets.
/// Containment is purely lexical (no filesystem access): `.` is dropped and
/// `..` is resolved without touching the disk, so missing files and pending
/// writes assess correctly.
pub fn assess_write_safety(
    tool_name: &str,
    args: &serde_json::Value,
    writable_roots: &[String],
    cwd: &str,
) -> WriteSafetyDecision {
    if !is_write_tool(tool_name) {
        return WriteSafetyDecision::AskUser;
    }
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        // Malformed-but-promptable requests keep the existing flow.
        return WriteSafetyDecision::AskUser;
    };
    if writable_roots.is_empty() {
        return WriteSafetyDecision::AskUser;
    }

    let cwd = Path::new(cwd);
    let target = Path::new(path);
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else if cwd.as_os_str().is_empty() {
        // No working directory to resolve a relative target against.
        return WriteSafetyDecision::AskUser;
    } else {
        cwd.join(target)
    };
    let Some(target) = normalize(&target) else {
        return WriteSafetyDecision::Reject {
            reason: format!("cannot resolve write target `{path}`"),
        };
    };

    let mut roots: Vec<PathBuf> = Vec::new();
    for root in writable_roots {
        let root = Path::new(root);
        let root = if root.is_absolute() {
            root.to_path_buf()
        } else if cwd.as_os_str().is_empty() {
            continue;
        } else {
            cwd.join(root)
        };
        if let Some(root) = normalize(&root) {
            roots.push(root);
        }
    }
    if roots.is_empty() {
        return WriteSafetyDecision::AskUser;
    }

    if roots.iter().any(|root| target.starts_with(root)) {
        WriteSafetyDecision::AutoApprove
    } else {
        WriteSafetyDecision::Reject {
            reason: format!("write target `{path}` is outside the sandbox writable roots"),
        }
    }
}

/// Lexically normalize a path: drop `.` components and resolve `..` without
/// touching the filesystem (works even when the path does not exist).
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Vec<String> {
        vec![
            "/Users/biu/Projects/piko".to_string(),
            "/Users/biu/Projects/piko/docs".to_string(),
        ]
    }

    fn args(path: &str) -> serde_json::Value {
        serde_json::json!({ "path": path })
    }

    #[test]
    fn in_roots_write_is_auto_approved() {
        for path in [
            "src/lib.rs",
            "/Users/biu/Projects/piko/src/lib.rs",
            "/Users/biu/Projects/piko/docs/design/D-12-safety.md",
        ] {
            let decision =
                assess_write_safety("edit", &args(path), &roots(), "/Users/biu/Projects/piko");
            assert_eq!(decision, WriteSafetyDecision::AutoApprove, "path: {path}");
        }
    }

    #[test]
    fn out_of_roots_write_is_rejected_with_reason() {
        for path in [
            "/Users/biu/.ssh/config",
            "../../etc/passwd",
            "/Users/biu/Projects/other/file.rs",
            "/Users/biu/Projects/piko-secret/x",
        ] {
            let decision =
                assess_write_safety("write", &args(path), &roots(), "/Users/biu/Projects/piko");
            match decision {
                WriteSafetyDecision::Reject { reason } => {
                    assert!(
                        reason.contains(path),
                        "reason should name the target: {reason}"
                    );
                }
                other => panic!("expected Reject for {path}, got {other:?}"),
            }
        }
    }

    #[test]
    fn unassessable_requests_ask_the_user() {
        // Non-write tools.
        assert_eq!(
            assess_write_safety(
                "bash",
                &args("src/lib.rs"),
                &roots(),
                "/Users/biu/Projects/piko"
            ),
            WriteSafetyDecision::AskUser
        );
        // Missing or non-string path.
        assert_eq!(
            assess_write_safety(
                "edit",
                &serde_json::json!({}),
                &roots(),
                "/Users/biu/Projects/piko"
            ),
            WriteSafetyDecision::AskUser
        );
        assert_eq!(
            assess_write_safety(
                "write",
                &serde_json::json!({ "path": 42 }),
                &roots(),
                "/Users/biu/Projects/piko"
            ),
            WriteSafetyDecision::AskUser
        );
        // No roots.
        assert_eq!(
            assess_write_safety("edit", &args("src/lib.rs"), &[], "/Users/biu/Projects/piko"),
            WriteSafetyDecision::AskUser
        );
        // Relative target with unknown cwd.
        assert_eq!(
            assess_write_safety("edit", &args("src/lib.rs"), &roots(), ""),
            WriteSafetyDecision::AskUser
        );
    }

    #[test]
    fn parent_traversal_inside_root_is_still_contained() {
        // `docs/../src/lib.rs` normalizes inside the root.
        let decision = assess_write_safety(
            "edit",
            &args("docs/../src/lib.rs"),
            &roots(),
            "/Users/biu/Projects/piko",
        );
        assert_eq!(decision, WriteSafetyDecision::AutoApprove);
    }
}
