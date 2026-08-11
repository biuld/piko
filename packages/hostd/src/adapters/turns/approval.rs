use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalScope {
    Session,
    Workspace,
    Permanent,
}

pub fn compute_exec_fingerprint(args: &serde_json::Value, cwd: &Path) -> String {
    let command = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
    let authority = args
        .get("sandbox_permissions")
        .and_then(|v| v.as_str())
        .unwrap_or("use_default");
    let workdir = args.get("workdir").and_then(|v| v.as_str()).unwrap_or(".");
    let workdir = normalize_workdir(cwd, workdir);
    let reusable_prefix = args.get("prefix_rule").cloned();
    let scope = reusable_prefix.unwrap_or_else(|| serde_json::json!([command]));
    let additional = args
        .get("additional_permissions")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let identity = serde_json::json!({
        "authority": authority,
        "workdir": workdir,
        "scope": scope,
        "additional": additional,
    });
    format!(
        "exec_command:{}",
        serde_json::to_string(&identity).unwrap_or_default()
    )
}

/// Lexically resolve a `workdir` against the session cwd without touching the
/// filesystem: `.` components are dropped and `..` resolved, so the default
/// `"."`, `"./"`, and the absolute session cwd produce one fingerprint.
fn normalize_workdir(cwd: &Path, workdir: &str) -> String {
    let requested = Path::new(workdir);
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        cwd.join(requested)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.display().to_string()
}

/// Path-level fingerprint for file tools. A grant covers exactly the target
/// path (as written in the call), so approving `edit` for one file never
/// leaks to other paths — including hostd state under `.piko/`.
pub fn compute_path_fingerprint(tool_name: &str, args: &serde_json::Value) -> String {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    format!("{tool_name}:{path}")
}

pub fn compute_fingerprint(tool_name: &str, tool_args: &serde_json::Value, cwd: &Path) -> String {
    match tool_name {
        "exec_command" => compute_exec_fingerprint(tool_args, cwd),
        "edit" | "write" | "read" => compute_path_fingerprint(tool_name, tool_args),
        _ => tool_name.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApproval {
    pub tool_name: String,
    pub fingerprint: String,
    pub sample_args: Option<serde_json::Value>,
    pub granted_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalsFile {
    pub fingerprints: HashMap<String, StoredApproval>,
}

fn read_approvals_file(path: &Path) -> ApprovalsFile {
    if !path.exists() {
        return ApprovalsFile::default();
    }
    let Ok(content) = fs::read_to_string(path) else {
        return ApprovalsFile::default();
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) else {
        return ApprovalsFile::default();
    };
    // Migrate old format: tools[toolName] -> fingerprints[fingerprint]
    if data.get("tools").is_some() && data.get("fingerprints").is_none() {
        return ApprovalsFile::default();
    }
    serde_json::from_value(data).unwrap_or_default()
}

fn write_approvals_file(path: &Path, data: &ApprovalsFile) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(data) {
        let _ = fs::write(path, serialized);
    }
}

pub struct ApprovalStore {
    session_approvals: Mutex<HashMap<String, StoredApproval>>,
    workspace_path: PathBuf,
    permanent_path: PathBuf,
    cwd: PathBuf,
}

impl ApprovalStore {
    pub fn new(cwd: &str) -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let cwd_path = Path::new(cwd)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(cwd));
        let workspace_path = cwd_path.join(".piko").join("approvals.json");
        let permanent_path = home_dir.join(".piko").join("approvals.json");

        Self {
            session_approvals: Mutex::new(HashMap::new()),
            workspace_path,
            permanent_path,
            cwd: cwd_path,
        }
    }

    pub fn is_approved(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> Option<ApprovalScope> {
        let fp = compute_fingerprint(tool_name, tool_args, &self.cwd);

        // 1. Session scope
        {
            let session = self.session_approvals.lock().unwrap();
            if session.contains_key(&fp) {
                return Some(ApprovalScope::Session);
            }
        }

        // 2. Workspace scope
        {
            let workspace = read_approvals_file(&self.workspace_path);
            if workspace.fingerprints.contains_key(&fp) {
                return Some(ApprovalScope::Workspace);
            }
        }

        // 3. Permanent scope
        {
            let permanent = read_approvals_file(&self.permanent_path);
            if permanent.fingerprints.contains_key(&fp) {
                return Some(ApprovalScope::Permanent);
            }
        }

        None
    }

    pub fn grant(&self, tool_name: &str, tool_args: &serde_json::Value, scope: ApprovalScope) {
        let fp = compute_fingerprint(tool_name, tool_args, &self.cwd);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let entry = StoredApproval {
            tool_name: tool_name.to_string(),
            fingerprint: fp.clone(),
            sample_args: Some(tool_args.clone()),
            granted_at: now,
        };

        match scope {
            ApprovalScope::Session => {
                let mut session = self.session_approvals.lock().unwrap();
                session.insert(fp, entry);
            }
            ApprovalScope::Workspace => {
                let mut workspace = read_approvals_file(&self.workspace_path);
                workspace.fingerprints.insert(fp.clone(), entry);
                write_approvals_file(&self.workspace_path, &workspace);
            }
            ApprovalScope::Permanent => {
                let mut permanent = read_approvals_file(&self.permanent_path);
                permanent.fingerprints.insert(fp.clone(), entry);
                write_approvals_file(&self.permanent_path, &permanent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_fingerprint_is_exact_unless_a_prefix_is_explicit() {
        let cwd = std::path::Path::new("/repo");
        let first = serde_json::json!({ "cmd": "git status" });
        let second = serde_json::json!({ "cmd": "git diff" });
        assert_ne!(
            compute_exec_fingerprint(&first, cwd),
            compute_exec_fingerprint(&second, cwd)
        );

        let prefixed_first = serde_json::json!({
            "cmd": "git status --short",
            "sandbox_permissions": "require_escalated",
            "prefix_rule": ["git", "status"]
        });
        let prefixed_second = serde_json::json!({
            "cmd": "git status --porcelain",
            "sandbox_permissions": "require_escalated",
            "prefix_rule": ["git", "status"]
        });
        assert_eq!(
            compute_exec_fingerprint(&prefixed_first, cwd),
            compute_exec_fingerprint(&prefixed_second, cwd)
        );

        let other_workdir = serde_json::json!({ "cmd": "git status", "workdir": "src" });
        assert_ne!(
            compute_exec_fingerprint(&first, cwd),
            compute_exec_fingerprint(&other_workdir, cwd)
        );
    }

    #[test]
    fn exec_fingerprint_normalizes_workdir_against_session_cwd() {
        let cwd = std::path::Path::new("/repo/sub");
        let default = serde_json::json!({ "cmd": "git status" });
        let explicit = serde_json::json!({ "cmd": "git status", "workdir": "/repo/sub" });
        let relative_dot = serde_json::json!({ "cmd": "git status", "workdir": "./" });
        let parent_back = serde_json::json!({ "cmd": "git status", "workdir": "../sub" });

        assert_eq!(
            compute_exec_fingerprint(&default, cwd),
            compute_exec_fingerprint(&explicit, cwd)
        );
        assert_eq!(
            compute_exec_fingerprint(&default, cwd),
            compute_exec_fingerprint(&relative_dot, cwd)
        );
        assert_eq!(
            compute_exec_fingerprint(&default, cwd),
            compute_exec_fingerprint(&parent_back, cwd)
        );
        // A genuinely different workdir stays distinct.
        let sibling = serde_json::json!({ "cmd": "git status", "workdir": "../other" });
        assert_ne!(
            compute_exec_fingerprint(&default, cwd),
            compute_exec_fingerprint(&sibling, cwd)
        );
    }

    #[test]
    fn test_compute_path_fingerprint() {
        let cwd = std::path::Path::new("/repo");
        let edit = serde_json::json!({ "path": "src/lib.rs", "edits": [] });
        assert_eq!(compute_fingerprint("edit", &edit, cwd), "edit:src/lib.rs");

        let write = serde_json::json!({ "path": "/abs/out.md", "content": "x" });
        assert_eq!(
            compute_fingerprint("write", &write, cwd),
            "write:/abs/out.md"
        );

        // A grant for one path never covers another path.
        assert_ne!(
            compute_fingerprint("edit", &serde_json::json!({ "path": "src/a.rs" }), cwd),
            compute_fingerprint("edit", &serde_json::json!({ "path": "src/b.rs" }), cwd)
        );
    }

    #[test]
    fn test_approval_store_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().to_str().unwrap();
        let store = ApprovalStore::new(cwd);

        let args = serde_json::json!({ "cmd": "git status" });

        // Initially not approved
        assert_eq!(store.is_approved("exec_command", &args), None);

        // Grant session
        store.grant("exec_command", &args, ApprovalScope::Session);
        assert_eq!(
            store.is_approved("exec_command", &args),
            Some(ApprovalScope::Session)
        );

        // Create new store in same cwd (session is cleared, but workspace/permanent remains)
        let store2 = ApprovalStore::new(cwd);
        assert_eq!(store2.is_approved("exec_command", &args), None);

        // Grant workspace
        store2.grant("exec_command", &args, ApprovalScope::Workspace);
        assert_eq!(
            store2.is_approved("exec_command", &args),
            Some(ApprovalScope::Workspace)
        );

        // New store in same cwd should load workspace approval
        let store3 = ApprovalStore::new(cwd);
        assert_eq!(
            store3.is_approved("exec_command", &args),
            Some(ApprovalScope::Workspace)
        );
    }
}
