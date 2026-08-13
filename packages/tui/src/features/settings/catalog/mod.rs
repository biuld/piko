//! Value-first Settings catalog assembled from host and TUI mirrors.

mod context;
mod presentation;
mod tools;

use super::mirror::{SettingsSnapshot, observability_summary, on_off};
use crate::ui::components::menu::{MenuRow, MenuRowKind};

/// Action applied when a setting option is confirmed.
#[derive(Clone, Debug)]
pub enum SettingsAction {
    Thinking(&'static str),
    HideThinking(bool),
    PromptCache(&'static str),
    Compaction(bool),
    CompactionKeep(u64),
    CompactionReserve(u64),
    CompactionMinGrowthFraction(f64),
    TranscriptMaxToolOutput(u64),
    Retry(bool),
    RetryMaxRetries(u32),
    RetryBaseDelay(u64),
    RetryMaxDelay(u64),
    RetryBudget(u64),
    ApprovalTimeout(u64),
    Guardian(bool),
    GuardianTimeout(u64),
    GuardianMaxDenials(u32),
    SafeWorkspaceWrites(bool),
    PermissionProfile(String),
    Feature(&'static str, bool),
    McpConnectTimeout(u64),
    EnableAllTools,
    DisableTools,
    Observability(bool),
    ObservabilityCaptureContent(bool),
    ObservabilityEndpoint(&'static str),
    Theme(&'static str),
    EditorMultiline(bool),
    EditorAutoResize(bool),
    EditorMaxLines(u16),
    EditorHistoryLimit(usize),
    TreeFilter(&'static str),
    BottomBarPreset(&'static str),
    Transport(&'static str),
}

pub(super) const GROUP_MODEL: &str = "Model";
pub(super) const GROUP_CONTEXT: &str = "Context";
pub(super) const GROUP_TRUST: &str = "Trust";
pub(super) const GROUP_TOOLS: &str = "Tools";
pub(super) const GROUP_DIAGNOSTICS: &str = "Diagnostics";
pub(super) const GROUP_APPEARANCE: &str = "Appearance";
pub(super) const GROUP_ADVANCED: &str = "Advanced";

pub(super) fn option_row(
    label: &str,
    detail: &str,
    action: SettingsAction,
    is_active: bool,
) -> MenuRow<SettingsAction> {
    MenuRow {
        title: label.into(),
        detail: detail.into(),
        value: None,
        badge: None,
        group: None,
        is_active,
        kind: MenuRowKind::Action(action),
    }
}

pub(super) fn binary_options(
    current: bool,
    on_detail: &str,
    off_detail: &str,
    on_action: SettingsAction,
    off_action: SettingsAction,
) -> Vec<MenuRow<SettingsAction>> {
    vec![
        option_row("On", on_detail, on_action, current),
        option_row("Off", off_detail, off_action, !current),
    ]
}

pub(super) fn section_choice(
    title: &str,
    summary: String,
    badge: Option<&'static str>,
    group: Option<&str>,
    choice_title: &str,
    options: Vec<MenuRow<SettingsAction>>,
) -> MenuRow<SettingsAction> {
    MenuRow {
        title: title.into(),
        detail: String::new(),
        value: Some(summary),
        badge: badge.map(str::to_string),
        group: group.map(str::to_string),
        is_active: false,
        kind: MenuRowKind::Choice {
            title: choice_title.into(),
            options,
        },
    }
}

pub(super) fn section_branch(
    title: &str,
    summary: String,
    badge: Option<&'static str>,
    group: Option<&str>,
    children: Vec<MenuRow<SettingsAction>>,
) -> MenuRow<SettingsAction> {
    MenuRow {
        title: title.into(),
        detail: String::new(),
        value: Some(summary),
        badge: badge.map(str::to_string),
        group: group.map(str::to_string),
        is_active: false,
        kind: MenuRowKind::Branch(children),
    }
}

/// Full Settings catalog rooted as domain-chunked sections.
pub fn build_catalog(snap: &SettingsSnapshot) -> Vec<MenuRow<SettingsAction>> {
    let host = &snap.host;
    let thinking = snap
        .thinking_level
        .as_deref()
        .or(host.thinking_level.as_deref());
    let thinking_options = ["off", "minimal", "low", "medium", "high", "xhigh", "max"]
        .iter()
        .map(|level| {
            option_row(
                level,
                thinking_level_detail(level),
                SettingsAction::Thinking(level),
                thinking == Some(*level),
            )
        })
        .collect();

    let mut rows = vec![
        section_choice(
            "Thinking Level",
            thinking.unwrap_or("—").to_string(),
            None,
            Some(GROUP_MODEL),
            "Thinking Level",
            thinking_options,
        ),
        presentation::prompt_cache(snap),
    ];
    rows.extend(context::rows(snap));
    rows.extend(tools::trust_rows(snap));
    rows.extend(tools::tool_rows(snap));
    rows.push(observability(snap));
    rows.extend(presentation::rows(snap));
    rows.push(section_choice(
        "Transport",
        host.transport.clone().unwrap_or_else(|| "stdio".into()),
        Some("restart hostd"),
        Some(GROUP_ADVANCED),
        "Transport",
        vec![option_row(
            "stdio",
            "JSON-lines over stdio; restart hostd to apply",
            SettingsAction::Transport("stdio"),
            host.transport
                .as_deref()
                .is_none_or(|value| value == "stdio"),
        )],
    ));
    rows
}

fn observability(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    section_branch(
        "Observability",
        observability_summary(host),
        Some("restart hostd"),
        Some(GROUP_DIAGNOSTICS),
        vec![
            section_choice(
                "OTLP Export",
                on_off(host.observability_enabled).into(),
                Some("restart hostd"),
                None,
                "OTLP Export",
                binary_options(
                    host.observability_enabled,
                    "Export telemetry after hostd restarts",
                    "Use stderr only after hostd restarts",
                    SettingsAction::Observability(true),
                    SettingsAction::Observability(false),
                ),
            ),
            section_choice(
                "GenAI Content",
                on_off(host.observability_capture_content).into(),
                Some("sensitive · restart hostd"),
                None,
                "GenAI Content",
                binary_options(
                    host.observability_capture_content,
                    "Export prompt, transcript, and tool-definition bodies",
                    "Export metadata only",
                    SettingsAction::ObservabilityCaptureContent(true),
                    SettingsAction::ObservabilityCaptureContent(false),
                ),
            ),
            presentation::otel_endpoint(snap),
        ],
    )
}

pub fn thinking_level_detail(level: &str) -> &'static str {
    match level {
        "off" => "Disable assistant thinking/reasoning",
        "minimal" => "Minimal reasoning budget",
        "low" => "Low reasoning budget",
        "medium" => "Medium reasoning budget",
        "high" => "High reasoning budget",
        "xhigh" => "Extra high reasoning budget",
        "max" => "Maximum reasoning budget",
        _ => "Reasoning budget",
    }
}

/// Whether this action requires a restart-hostd notice after apply.
pub fn action_requires_hostd_restart(action: &SettingsAction) -> bool {
    matches!(
        action,
        SettingsAction::Observability(_)
            | SettingsAction::ObservabilityCaptureContent(_)
            | SettingsAction::ObservabilityEndpoint(_)
            | SettingsAction::Transport(_)
    )
}
