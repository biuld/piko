use super::*;
use crate::features::settings::mirror::{feature_summary, guardian_summary};

const FEATURES: &[(&str, &str)] = &[
    ("workspace", "Workspace read/edit/write tools"),
    ("exec", "Command execution and live sessions"),
    ("environment", "Environment discovery"),
    ("context", "Context budget tools"),
    ("todo", "Task-list planning"),
    ("multi-agent", "Agent supervision and messaging"),
    ("user-interaction", "Structured user questions"),
    ("mcp", "Configured MCP server tools"),
];

pub(super) fn trust_rows(snap: &SettingsSnapshot) -> Vec<MenuRow<SettingsAction>> {
    let host = &snap.host;
    vec![
        section_choice(
            "Approval Timeout",
            format!("{}s", host.approval_timeout_secs),
            None,
            Some(GROUP_TRUST),
            "Approval Timeout",
            [30, 120, 300]
                .into_iter()
                .map(|value| {
                    option_row(
                        &format!("{value}s"),
                        "How long a user approval may remain pending",
                        SettingsAction::ApprovalTimeout(value),
                        host.approval_timeout_secs == value,
                    )
                })
                .collect(),
        ),
        guardian(snap),
        section_choice(
            "Safe Workspace Writes",
            on_off(host.safe_workspace_writes).into(),
            None,
            Some(GROUP_TRUST),
            "Safe Workspace Writes",
            binary_options(
                host.safe_workspace_writes,
                "Auto-approve writes constrained by writable roots",
                "Route constrained writes through normal approval",
                SettingsAction::SafeWorkspaceWrites(true),
                SettingsAction::SafeWorkspaceWrites(false),
            ),
        ),
    ]
}

pub(super) fn tool_rows(snap: &SettingsSnapshot) -> Vec<MenuRow<SettingsAction>> {
    let host = &snap.host;
    let tools_all = host.all_tools && !snap.no_tools;
    vec![
        section_choice(
            "Active Tools",
            if tools_all {
                "all".into()
            } else {
                "none".into()
            },
            None,
            Some(GROUP_TOOLS),
            "Active Tools",
            vec![
                option_row(
                    "All",
                    "Allow all discovered tools",
                    SettingsAction::EnableAllTools,
                    tools_all,
                ),
                option_row(
                    "None",
                    "Hide every tool from model runs",
                    SettingsAction::DisableTools,
                    !tools_all,
                ),
            ],
        ),
        permission_profile(snap),
        feature_gates(snap),
        section_choice(
            "MCP Connect Timeout",
            format!("{}s", host.mcp_connect_timeout_ms / 1000),
            None,
            Some(GROUP_TOOLS),
            "MCP Connect Timeout",
            [5_000, 10_000, 30_000]
                .into_iter()
                .map(|value| {
                    option_row(
                        &format!("{}s", value / 1000),
                        "Per-server startup connection deadline",
                        SettingsAction::McpConnectTimeout(value),
                        host.mcp_connect_timeout_ms == value,
                    )
                })
                .collect(),
        ),
    ]
}

fn guardian(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    section_branch(
        "Guardian Review",
        guardian_summary(host),
        None,
        Some(GROUP_TRUST),
        vec![
            section_choice(
                "Enabled",
                on_off(host.guardian_enabled).into(),
                None,
                None,
                "Guardian Review",
                binary_options(
                    host.guardian_enabled,
                    "Review tool approvals with the configured model",
                    "Send approvals directly to the user",
                    SettingsAction::Guardian(true),
                    SettingsAction::Guardian(false),
                ),
            ),
            section_choice(
                "Review Timeout",
                format!("{}s", host.guardian_timeout_secs),
                None,
                None,
                "Guardian Timeout",
                [15, 30, 60]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &format!("{value}s"),
                            "Maximum time for one guardian review",
                            SettingsAction::GuardianTimeout(value),
                            host.guardian_timeout_secs == value,
                        )
                    })
                    .collect(),
            ),
            section_choice(
                "Circuit Breaker",
                format!("{} denials", host.guardian_max_consecutive_denials),
                None,
                None,
                "Guardian Circuit Breaker",
                [1, 3, 5]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &value.to_string(),
                            "Consecutive non-accepting reviews before escalation",
                            SettingsAction::GuardianMaxDenials(value),
                            host.guardian_max_consecutive_denials == value,
                        )
                    })
                    .collect(),
            ),
        ],
    )
}

fn permission_profile(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    let options = host
        .permission_profiles
        .iter()
        .map(|profile| {
            option_row(
                profile,
                "Apply this file/network/command policy to subsequent runs",
                SettingsAction::PermissionProfile(profile.clone()),
                host.permission_profile == *profile,
            )
        })
        .collect();
    section_choice(
        "Permission Profile",
        host.permission_profile.clone(),
        None,
        Some(GROUP_TOOLS),
        "Permission Profile",
        options,
    )
}

fn feature_gates(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    let children = FEATURES
        .iter()
        .filter(|(key, _)| !host.managed_features.contains_key(*key))
        .map(|(key, detail)| {
            let current = host.features.get(*key).copied().unwrap_or(true);
            section_choice(
                key,
                on_off(current).into(),
                None,
                None,
                key,
                binary_options(
                    current,
                    detail,
                    detail,
                    SettingsAction::Feature(key, true),
                    SettingsAction::Feature(key, false),
                ),
            )
        })
        .collect();
    section_branch(
        "Feature Gates",
        feature_summary(host),
        None,
        Some(GROUP_TOOLS),
        children,
    )
}
