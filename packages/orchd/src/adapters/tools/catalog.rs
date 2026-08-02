// ---- Tool catalog building and policy projection ----

use std::collections::HashSet;

use crate::domain::tools::definition::{
    ToolApprovalPolicy, ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolPolicy,
    ToolSensitivity, ToolSetPolicy, ToolSetToolRef,
};

#[derive(Debug, Clone)]
pub(super) struct CatalogEntry {
    pub(super) public_name: String,
    pub(super) provider_id: String,
    pub(super) provider_tool_name: String,
    pub(super) tool_def: ToolDef,
    pub(super) execution_mode: ToolExecutionMode,
    pub(super) max_concurrent_calls: Option<u32>,
}

pub(super) fn add_entry(
    entries: &mut Vec<CatalogEntry>,
    seen: &mut HashSet<String>,
    duplicates: &mut HashSet<String>,
    public_name: &str,
    provider_id: &str,
    provider_tool_name: &str,
    tool_def: &ToolDef,
    policy: Option<&ToolPolicy>,
    set_policy: Option<&ToolSetPolicy>,
) {
    if seen.contains(public_name) {
        duplicates.insert(public_name.to_string());
    }
    seen.insert(public_name.to_string());
    let projected = project_tool_def(tool_def, public_name, policy);
    let execution_mode = resolve_execution_mode(&projected, set_policy);
    let max_concurrent_calls = set_policy.and_then(|set| set.max_concurrent_calls);
    entries.push(CatalogEntry {
        public_name: public_name.to_string(),
        provider_id: provider_id.to_string(),
        provider_tool_name: provider_tool_name.to_string(),
        tool_def: projected,
        execution_mode,
        max_concurrent_calls,
    });
}

/// Resolve the effective execution mode for a catalog entry.
///
/// Resolution order per F-06: an explicit per-tool/per-policy mode wins; a
/// set-level `allowParallel: true` upgrades an unset mode; anything unset
/// defaults to `Sequential` (fail-closed, mirroring codex-rs).
pub(super) fn resolve_execution_mode(
    projected: &ToolDef,
    set_policy: Option<&ToolSetPolicy>,
) -> ToolExecutionMode {
    if let Some(ref mode) = projected.execution_mode {
        return mode.clone();
    }
    if set_policy
        .and_then(|set| set.allow_parallel)
        .unwrap_or(false)
    {
        return ToolExecutionMode::Parallel;
    }
    ToolExecutionMode::Sequential
}

/// Apply policy overrides to a tool definition.
pub(super) fn project_tool_def(
    tool_def: &ToolDef,
    public_name: &str,
    policy: Option<&ToolPolicy>,
) -> ToolDef {
    let mut projected = tool_def.clone();
    projected.name = public_name.to_string();

    let Some(p) = policy else {
        return projected;
    };

    // Apply approval policy
    if let Some(ref approval_policy) = p.approval {
        projected.approval = match approval_policy {
            ToolApprovalPolicy::Never => Some(ToolApprovalRequirement::Never),
            ToolApprovalPolicy::OnSensitive => {
                // Keep existing if set, otherwise on_request
                if projected.approval.is_none() {
                    Some(ToolApprovalRequirement::OnRequest)
                } else {
                    projected.approval
                }
            }
            ToolApprovalPolicy::Always => Some(ToolApprovalRequirement::Always),
        };
    } else if let Some(ref sensitivity) = p.sensitivity {
        projected.approval = match sensitivity {
            ToolSensitivity::Safe => projected.approval,
            ToolSensitivity::Sensitive
                if projected
                    .approval
                    .as_ref()
                    .is_some_and(|a| *a == ToolApprovalRequirement::Never) =>
            {
                Some(ToolApprovalRequirement::OnRequest)
            }
            ToolSensitivity::Dangerous => Some(ToolApprovalRequirement::Always),
            ToolSensitivity::Dynamic => projected.approval,
            _ => projected.approval,
        };
    }

    // Apply execution mode
    if let Some(ref mode) = p.execution_mode {
        projected.execution_mode = Some(mode.clone());
    }

    projected
}

/// Extract policy from a tool set reference.
pub(super) fn tool_ref_policy(tool_ref: &ToolSetToolRef) -> Option<&ToolPolicy> {
    match tool_ref {
        ToolSetToolRef::ProviderTool { policy, .. }
        | ToolSetToolRef::ProviderNamespace { policy, .. }
        | ToolSetToolRef::OrchestratorControl { policy, .. } => policy.as_ref(),
    }
}

/// Merge tool set defaults with per-tool policy.
pub(super) fn merge_policy(
    tool_set_policy: Option<&ToolSetPolicy>,
    tool_policy: Option<&ToolPolicy>,
) -> Option<ToolPolicy> {
    match (tool_set_policy, tool_policy) {
        (None, None) => None,
        (Some(tsp), None) => tsp.defaults.clone(),
        (None, Some(tp)) => Some(tp.clone()),
        (Some(tsp), Some(tp)) => {
            let mut merged = tsp.defaults.clone().unwrap_or_default();
            if tp.sensitivity.is_some() {
                merged.sensitivity = tp.sensitivity.clone();
            }
            if tp.approval.is_some() {
                merged.approval = tp.approval.clone();
            }
            if tp.timeout_ms.is_some() {
                merged.timeout_ms = tp.timeout_ms;
            }
            if tp.execution_mode.is_some() {
                merged.execution_mode = tp.execution_mode.clone();
            }
            if tp.failure_mode.is_some() {
                merged.failure_mode = tp.failure_mode.clone();
            }
            Some(merged)
        }
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_project_tool_def_never_approval() {
        let tool = ToolDef {
            name: "test_tool".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test-tool", "test_tool"),
            description: "".into(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: "native".into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        };

        let policy = ToolPolicy {
            approval: Some(ToolApprovalPolicy::Never),
            ..Default::default()
        };

        let projected = project_tool_def(&tool, "test_tool", Some(&policy));
        assert_eq!(projected.approval, Some(ToolApprovalRequirement::Never));
    }

    #[tokio::test]
    async fn test_project_tool_def_dangerous_sensitivity() {
        let tool = ToolDef {
            name: "dangerous_tool".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test-tool", "dangerous_tool"),
            description: "".into(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: "native".into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        };

        let policy = ToolPolicy {
            sensitivity: Some(ToolSensitivity::Dangerous),
            ..Default::default()
        };

        let projected = project_tool_def(&tool, "dangerous_tool", Some(&policy));
        assert_eq!(projected.approval, Some(ToolApprovalRequirement::Always));
    }

    #[test]
    fn resolve_execution_mode_prefers_explicit_mode() {
        let mut tool = timing_tool_def("explicit");
        tool.execution_mode = Some(ToolExecutionMode::Parallel);
        let set_policy = ToolSetPolicy {
            defaults: None,
            allow_parallel: Some(false),
            max_concurrent_calls: None,
        };
        assert_eq!(
            resolve_execution_mode(&tool, Some(&set_policy)),
            ToolExecutionMode::Parallel
        );
    }

    #[test]
    fn resolve_execution_mode_upgrades_unset_mode_via_allow_parallel() {
        let tool = timing_tool_def("upgraded");
        let set_policy = ToolSetPolicy {
            defaults: None,
            allow_parallel: Some(true),
            max_concurrent_calls: None,
        };
        assert_eq!(
            resolve_execution_mode(&tool, Some(&set_policy)),
            ToolExecutionMode::Parallel
        );
    }

    #[test]
    fn resolve_execution_mode_defaults_fail_closed_to_sequential() {
        let tool = timing_tool_def("default");
        assert_eq!(
            resolve_execution_mode(&tool, None),
            ToolExecutionMode::Sequential
        );
        let set_policy = ToolSetPolicy {
            defaults: None,
            allow_parallel: Some(false),
            max_concurrent_calls: None,
        };
        assert_eq!(
            resolve_execution_mode(&tool, Some(&set_policy)),
            ToolExecutionMode::Sequential
        );
    }

    fn timing_tool_def(name: &str) -> ToolDef {
        ToolDef {
            name: name.into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test-tool", name),
            description: "".into(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: "native".into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        }
    }
}
