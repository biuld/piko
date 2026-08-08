//! Static Settings catalog → SettingSection tree from a snapshot.
//!
//! Root is a **flat value-first list** chunked into domain groups (headers painted
//! by the kit). Nested branches only where a composite needs more than one key.

use super::mirror::{
    SettingsSnapshot, compaction_summary, observability_summary, on_off, otel_endpoint_is_custom,
    otel_endpoint_preset_active,
};
use crate::ui::components::setting::{
    EffectClass, SettingBody, SettingChoiceList, SettingOption, SettingSection,
};

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

fn binary_choice(
    title: &str,
    effect: EffectClass,
    current: bool,
    on_detail: &str,
    off_detail: &str,
    on_action: SettingsAction,
    off_action: SettingsAction,
) -> SettingChoiceList<SettingsAction> {
    SettingChoiceList {
        title: title.to_string(),
        effect,
        options: vec![
            SettingOption {
                label: "On".into(),
                detail: on_detail.into(),
                action: on_action,
                is_active: current,
            },
            SettingOption {
                label: "Off".into(),
                detail: off_detail.into(),
                action: off_action,
                is_active: !current,
            },
        ],
    }
}

fn section_choice(
    title: &str,
    summary: String,
    effect: EffectClass,
    group: Option<&str>,
    choice: SettingChoiceList<SettingsAction>,
) -> SettingSection<SettingsAction> {
    SettingSection {
        title: title.to_string(),
        value_summary: summary,
        effect,
        group: group.map(str::to_string),
        body: SettingBody::Choice(choice),
    }
}

fn section_branch(
    title: &str,
    summary: String,
    effect: EffectClass,
    group: Option<&str>,
    children: Vec<SettingSection<SettingsAction>>,
) -> SettingSection<SettingsAction> {
    SettingSection {
        title: title.to_string(),
        value_summary: summary,
        effect,
        group: group.map(str::to_string),
        body: SettingBody::Branch(children),
    }
}

/// Full Settings catalog rooted as domain-chunked sections.
pub fn build_catalog(snap: &SettingsSnapshot) -> Vec<SettingSection<SettingsAction>> {
    let host = &snap.host;
    let thinking = snap
        .thinking_level
        .as_deref()
        .or(host.thinking_level.as_deref());

    let thinking_levels = ["off", "minimal", "low", "medium", "high", "xhigh"];
    let thinking_choice = SettingChoiceList {
        title: "Thinking Level".into(),
        effect: EffectClass::Live,
        options: thinking_levels
            .iter()
            .map(|level| SettingOption {
                label: (*level).into(),
                detail: thinking_level_detail(level).into(),
                action: SettingsAction::Thinking(level),
                is_active: thinking == Some(*level),
            })
            .collect(),
    };

    let thinking_blocks = SettingChoiceList {
        title: "Thinking Blocks".into(),
        effect: EffectClass::Presentation,
        options: vec![
            SettingOption {
                label: "Shown".into(),
                detail: "Show thinking content in the timeline where supported".into(),
                action: SettingsAction::HideThinking(false),
                is_active: snap.thinking_visible,
            },
            SettingOption {
                label: "Hidden".into(),
                detail: "Hide thinking content in future rendering".into(),
                action: SettingsAction::HideThinking(true),
                is_active: !snap.thinking_visible,
            },
        ],
    };

    let compaction_enable = binary_choice(
        "Automatic Compaction",
        EffectClass::Live,
        host.compaction_enabled,
        "Enable hostd automatic compaction",
        "Disable hostd automatic compaction",
        SettingsAction::Compaction(true),
        SettingsAction::Compaction(false),
    );

    let reserve_opts: &[(u64, &str)] = &[(8192, "8k"), (16384, "16k (default)"), (32768, "32k")];
    let reserve_choice = SettingChoiceList {
        title: "Reserve Tokens".into(),
        effect: EffectClass::Live,
        options: reserve_opts
            .iter()
            .map(|(n, label)| SettingOption {
                label: (*label).into(),
                detail: format!("Reserve {n} tokens for system context"),
                action: SettingsAction::CompactionReserve(*n),
                is_active: host.compaction_reserve == *n,
            })
            .collect(),
    };

    let keep_opts: &[(u64, &str)] = &[
        (10000, "10k"),
        (20000, "20k (default)"),
        (30000, "30k"),
        (50000, "50k"),
    ];
    let keep_choice = SettingChoiceList {
        title: "Keep Recent Tokens".into(),
        effect: EffectClass::Live,
        options: keep_opts
            .iter()
            .map(|(n, label)| SettingOption {
                label: (*label).into(),
                detail: format!("Keep {n} recent tokens intact"),
                action: SettingsAction::CompactionKeep(*n),
                is_active: host.compaction_keep == *n,
            })
            .collect(),
    };

    let compaction_branch = section_branch(
        "Compaction",
        compaction_summary(host),
        EffectClass::Live,
        Some(GROUP_CONTEXT),
        vec![
            section_choice(
                "Enable",
                on_off(host.compaction_enabled).into(),
                EffectClass::Live,
                None,
                compaction_enable,
            ),
            section_choice(
                "Reserve Tokens",
                format_reserve(host.compaction_reserve),
                EffectClass::Live,
                None,
                reserve_choice,
            ),
            section_choice(
                "Keep Recent Tokens",
                format_keep(host.compaction_keep),
                EffectClass::Live,
                None,
                keep_choice,
            ),
        ],
    );

    let mut otel_options: Vec<SettingOption<SettingsAction>> = vec![
        SettingOption {
            label: "Local collector".into(),
            detail: "http://127.0.0.1:4318 — Aspire / Jaeger / OTel collector".into(),
            action: SettingsAction::ObservabilityEndpoint("http://127.0.0.1:4318"),
            is_active: otel_endpoint_preset_active(host, "http://127.0.0.1:4318"),
        },
        SettingOption {
            label: "Localhost".into(),
            detail: "http://localhost:4318".into(),
            action: SettingsAction::ObservabilityEndpoint("http://localhost:4318"),
            is_active: otel_endpoint_preset_active(host, "http://localhost:4318"),
        },
    ];
    if otel_endpoint_is_custom(host, OTEL_PRESETS) {
        // Custom value stays on the summary line only; no Active preset.
        let _ = &mut otel_options;
    }

    let observability_branch = section_branch(
        "Observability",
        observability_summary(host),
        EffectClass::RestartHostd,
        Some(GROUP_DIAGNOSTICS),
        vec![
            section_choice(
                "OTLP export",
                on_off(host.observability_enabled).into(),
                EffectClass::RestartHostd,
                None,
                binary_choice(
                    "OTLP export",
                    EffectClass::RestartHostd,
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
                EffectClass::RestartHostd,
                None,
                SettingChoiceList {
                    title: "OTLP Endpoint".into(),
                    effect: EffectClass::RestartHostd,
                    options: otel_options,
                },
            ),
        ],
    );

    let tools_all = host.all_tools && !snap.no_tools;

    vec![
        // ── Thinking ──────────────────────────────────────────────────────
        section_choice(
            "Level",
            thinking.unwrap_or("—").to_string(),
            EffectClass::Live,
            Some(GROUP_THINKING),
            thinking_choice,
        ),
        section_choice(
            "Blocks",
            if snap.thinking_visible {
                "shown".into()
            } else {
                "hidden".into()
            },
            EffectClass::Presentation,
            Some(GROUP_THINKING),
            thinking_blocks,
        ),
        // ── Context ───────────────────────────────────────────────────────
        compaction_branch,
        section_choice(
            "API Retries",
            on_off(host.retry_enabled).into(),
            EffectClass::Live,
            Some(GROUP_CONTEXT),
            binary_choice(
                "API Retries",
                EffectClass::Live,
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
            EffectClass::Live,
            Some(GROUP_TOOLS),
            binary_choice(
                "Sandbox",
                EffectClass::Live,
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
            EffectClass::Live,
            Some(GROUP_TOOLS),
            SettingChoiceList {
                title: "Active Tools".into(),
                effect: EffectClass::Live,
                options: vec![
                    SettingOption {
                        label: "All tools".into(),
                        detail: "Allow all discovered tools".into(),
                        action: SettingsAction::EnableAllTools,
                        is_active: tools_all,
                    },
                    SettingOption {
                        label: "No tools".into(),
                        detail: "Set active tools to an empty list".into(),
                        action: SettingsAction::DisableTools,
                        is_active: !tools_all,
                    },
                ],
            },
        ),
        // ── Diagnostics ───────────────────────────────────────────────────
        observability_branch,
        // ── Appearance ────────────────────────────────────────────────────
        section_choice(
            "Theme",
            snap.theme_name.clone(),
            EffectClass::Presentation,
            Some(GROUP_APPEARANCE),
            SettingChoiceList {
                title: "Theme".into(),
                effect: EffectClass::Presentation,
                options: vec![
                    SettingOption {
                        label: "dark".into(),
                        detail: "Dark theme".into(),
                        action: SettingsAction::Theme("dark"),
                        is_active: snap.theme_name == "dark",
                    },
                    SettingOption {
                        label: "light".into(),
                        detail: "Light theme".into(),
                        action: SettingsAction::Theme("light"),
                        is_active: snap.theme_name == "light",
                    },
                ],
            },
        ),
        // ── Advanced ──────────────────────────────────────────────────────
        section_choice(
            "Transport",
            host.transport.clone().unwrap_or_else(|| "stdio".into()),
            EffectClass::Live,
            Some(GROUP_ADVANCED),
            SettingChoiceList {
                title: "Transport".into(),
                effect: EffectClass::Live,
                options: vec![SettingOption {
                    label: "stdio".into(),
                    detail: "Host transport preference (stdio)".into(),
                    action: SettingsAction::Transport("stdio"),
                    is_active: host
                        .transport
                        .as_deref()
                        .map(|t| t == "stdio")
                        .unwrap_or(true),
                }],
            },
        ),
    ]
}

/// Thinking Level choice list for the dedicated thinking picker.
pub fn build_thinking_choice(snap: &SettingsSnapshot) -> SettingChoiceList<SettingsAction> {
    let thinking = snap
        .thinking_level
        .as_deref()
        .or(snap.host.thinking_level.as_deref());
    let levels = ["off", "minimal", "low", "medium", "high", "xhigh"];
    SettingChoiceList {
        title: "Thinking Level".into(),
        effect: EffectClass::Live,
        options: levels
            .iter()
            .map(|level| SettingOption {
                label: (*level).into(),
                detail: thinking_level_detail(level).into(),
                action: SettingsAction::Thinking(level),
                is_active: thinking == Some(*level),
            })
            .collect(),
    }
}

fn thinking_level_detail(level: &str) -> &'static str {
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
