//! Static Settings catalog → [`MenuRow`] tree from a snapshot.
//!
//! Root is a **flat value-first list** chunked into domain groups (headers painted
//! by the shared menu rows). Nested branches only where a composite needs more
//! than one key.

use super::mirror::{
    SettingsSnapshot, compaction_summary, observability_summary, on_off, otel_endpoint_is_custom,
    otel_endpoint_preset_active,
};
use crate::ui::components::menu::{MenuRow, MenuRowKind};

/// Action applied when a setting option is confirmed.
#[derive(Clone, Debug)]
pub enum SettingsAction {
    Thinking(&'static str),
    HideThinking(bool),
    Compaction(bool),
    CompactionKeep(u64),
    CompactionReserve(u64),
    Theme(&'static str),
    Transport(&'static str),
    Sandbox(bool),
    Retry(bool),
    Observability(bool),
    ObservabilityEndpoint(&'static str),
    EnableAllTools,
    DisableTools,
}

const OTEL_PRESETS: &[&str] = &["http://127.0.0.1:4318", "http://localhost:4318"];

// Domain chunks on the catalog root (product labels).
const GROUP_THINKING: &str = "Thinking";
const GROUP_CONTEXT: &str = "Context";
const GROUP_TOOLS: &str = "Tools";
const GROUP_DIAGNOSTICS: &str = "Diagnostics";
const GROUP_APPEARANCE: &str = "Appearance";
const GROUP_ADVANCED: &str = "Advanced";

fn option_row(
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

fn binary_options(
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

fn section_choice(
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

fn section_branch(
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

    let thinking_levels = ["off", "minimal", "low", "medium", "high", "xhigh"];
    let thinking_options: Vec<MenuRow<SettingsAction>> = thinking_levels
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

    let thinking_blocks_options = vec![
        option_row(
            "Shown",
            "Show thinking content in the timeline where supported",
            SettingsAction::HideThinking(false),
            snap.thinking_visible,
        ),
        option_row(
            "Hidden",
            "Hide thinking content in future rendering",
            SettingsAction::HideThinking(true),
            !snap.thinking_visible,
        ),
    ];

    let compaction_enable_options = binary_options(
        host.compaction_enabled,
        "Enable hostd automatic compaction",
        "Disable hostd automatic compaction",
        SettingsAction::Compaction(true),
        SettingsAction::Compaction(false),
    );

    let reserve_opts: &[(u64, &str)] = &[(8192, "8k"), (16384, "16k (default)"), (32768, "32k")];
    let reserve_options: Vec<MenuRow<SettingsAction>> = reserve_opts
        .iter()
        .map(|(n, label)| {
            option_row(
                label,
                &format!("Reserve {n} tokens for system context"),
                SettingsAction::CompactionReserve(*n),
                host.compaction_reserve == *n,
            )
        })
        .collect();

    let keep_opts: &[(u64, &str)] = &[
        (10000, "10k"),
        (20000, "20k (default)"),
        (30000, "30k"),
        (50000, "50k"),
    ];
    let keep_options: Vec<MenuRow<SettingsAction>> = keep_opts
        .iter()
        .map(|(n, label)| {
            option_row(
                label,
                &format!("Keep {n} recent tokens intact"),
                SettingsAction::CompactionKeep(*n),
                host.compaction_keep == *n,
            )
        })
        .collect();

    let compaction_branch = section_branch(
        "Compaction",
        compaction_summary(host),
        None,
        Some(GROUP_CONTEXT),
        vec![
            section_choice(
                "Enable",
                on_off(host.compaction_enabled).into(),
                None,
                None,
                "Automatic Compaction",
                compaction_enable_options,
            ),
            section_choice(
                "Reserve Tokens",
                format_reserve(host.compaction_reserve),
                None,
                None,
                "Reserve Tokens",
                reserve_options,
            ),
            section_choice(
                "Keep Recent Tokens",
                format_keep(host.compaction_keep),
                None,
                None,
                "Keep Recent Tokens",
                keep_options,
            ),
        ],
    );

    let otel_options: Vec<MenuRow<SettingsAction>> = vec![
        option_row(
            "Local collector",
            "http://127.0.0.1:4318 — Aspire / Jaeger / OTel collector",
            SettingsAction::ObservabilityEndpoint("http://127.0.0.1:4318"),
            otel_endpoint_preset_active(host, "http://127.0.0.1:4318"),
        ),
        option_row(
            "Localhost",
            "http://localhost:4318",
            SettingsAction::ObservabilityEndpoint("http://localhost:4318"),
            otel_endpoint_preset_active(host, "http://localhost:4318"),
        ),
    ];

    let observability_branch = section_branch(
        "Observability",
        observability_summary(host),
        Some("restart hostd"),
        Some(GROUP_DIAGNOSTICS),
        vec![
            section_choice(
                "OTLP export",
                on_off(host.observability_enabled).into(),
                Some("restart hostd"),
                None,
                "OTLP export",
                binary_options(
                    host.observability_enabled,
                    "Export traces/metrics/logs over OTLP HTTP (restart hostd to apply)",
                    "Stderr only when hostd starts (restart hostd to apply)",
                    SettingsAction::Observability(true),
                    SettingsAction::Observability(false),
                ),
            ),
            section_choice(
                "OTLP Endpoint",
                {
                    let mut s = host.otel_endpoint.clone();
                    if otel_endpoint_is_custom(host, OTEL_PRESETS) {
                        s.push_str(" · custom");
                    }
                    s
                },
                Some("restart hostd"),
                None,
                "OTLP Endpoint",
                otel_options,
            ),
        ],
    );

    let tools_all = host.all_tools && !snap.no_tools;

    vec![
        // ── Thinking ──────────────────────────────────────────────────────
        section_choice(
            "Level",
            thinking.unwrap_or("—").to_string(),
            None,
            Some(GROUP_THINKING),
            "Thinking Level",
            thinking_options,
        ),
        section_choice(
            "Blocks",
            if snap.thinking_visible {
                "shown".into()
            } else {
                "hidden".into()
            },
            None,
            Some(GROUP_THINKING),
            "Thinking Blocks",
            thinking_blocks_options,
        ),
        // ── Context ───────────────────────────────────────────────────────
        compaction_branch,
        section_choice(
            "API Retries",
            on_off(host.retry_enabled).into(),
            None,
            Some(GROUP_CONTEXT),
            "API Retries",
            binary_options(
                host.retry_enabled,
                "Automatic retries on LLM API failure",
                "No automatic retries",
                SettingsAction::Retry(true),
                SettingsAction::Retry(false),
            ),
        ),
        // ── Tools ─────────────────────────────────────────────────────────
        section_choice(
            "Sandbox",
            on_off(host.sandbox_enabled).into(),
            None,
            Some(GROUP_TOOLS),
            "Sandbox",
            binary_options(
                host.sandbox_enabled,
                "Filesystem & shell sandboxing",
                "Sandbox disabled",
                SettingsAction::Sandbox(true),
                SettingsAction::Sandbox(false),
            ),
        ),
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
                    "All tools",
                    "Allow all discovered tools",
                    SettingsAction::EnableAllTools,
                    tools_all,
                ),
                option_row(
                    "No tools",
                    "Set active tools to an empty list",
                    SettingsAction::DisableTools,
                    !tools_all,
                ),
            ],
        ),
        // ── Diagnostics ───────────────────────────────────────────────────
        observability_branch,
        // ── Appearance ────────────────────────────────────────────────────
        section_choice(
            "Theme",
            snap.theme_name.clone(),
            None,
            Some(GROUP_APPEARANCE),
            "Theme",
            vec![
                option_row(
                    "dark",
                    "Dark theme",
                    SettingsAction::Theme("dark"),
                    snap.theme_name == "dark",
                ),
                option_row(
                    "light",
                    "Light theme",
                    SettingsAction::Theme("light"),
                    snap.theme_name == "light",
                ),
            ],
        ),
        // ── Advanced ──────────────────────────────────────────────────────
        section_choice(
            "Transport",
            host.transport.clone().unwrap_or_else(|| "stdio".into()),
            None,
            Some(GROUP_ADVANCED),
            "Transport",
            vec![option_row(
                "stdio",
                "Host transport preference (stdio)",
                SettingsAction::Transport("stdio"),
                host.transport
                    .as_deref()
                    .map(|t| t == "stdio")
                    .unwrap_or(true),
            )],
        ),
    ]
}

pub fn thinking_level_detail(level: &str) -> &'static str {
    match level {
        "off" => "Disable assistant thinking/reasoning",
        "minimal" => "Minimal reasoning budget",
        "low" => "Low reasoning budget",
        "medium" => "Medium reasoning budget",
        "high" => "High reasoning budget",
        "xhigh" => "Extra high reasoning budget (maximum)",
        _ => "Reasoning budget",
    }
}

fn format_reserve(n: u64) -> String {
    if n >= 1024 && n.is_multiple_of(1024) {
        format!("{}k tokens", n / 1024)
    } else {
        format!("{n} tokens")
    }
}

fn format_keep(n: u64) -> String {
    if n >= 1000 && n.is_multiple_of(1000) {
        format!("{}k tokens", n / 1000)
    } else {
        format!("{n} tokens")
    }
}

/// Whether this action requires a restart-hostd notice after apply.
pub fn action_requires_hostd_restart(action: &SettingsAction) -> bool {
    matches!(
        action,
        SettingsAction::Observability(_) | SettingsAction::ObservabilityEndpoint(_)
    )
}
