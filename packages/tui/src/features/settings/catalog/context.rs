use super::*;
use crate::features::settings::mirror::compaction_summary;

pub(super) fn rows(snap: &SettingsSnapshot) -> Vec<MenuRow<SettingsAction>> {
    let host = &snap.host;
    vec![
        compaction(snap),
        section_choice(
            "Tool Output Limit",
            tokens(host.transcript_max_tool_output_tokens),
            None,
            Some(GROUP_CONTEXT),
            "Tool Output Limit",
            numeric_options(
                &[8_000, 24_000, 48_000],
                host.transcript_max_tool_output_tokens,
                "tokens per tool result in model context",
                SettingsAction::TranscriptMaxToolOutput,
            ),
        ),
        retry(snap),
    ]
}

fn compaction(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    section_branch(
        "Compaction",
        compaction_summary(host),
        None,
        Some(GROUP_CONTEXT),
        vec![
            section_choice(
                "Automatic Compaction",
                on_off(host.compaction_enabled).into(),
                None,
                None,
                "Automatic Compaction",
                binary_options(
                    host.compaction_enabled,
                    "Compact context automatically",
                    "Require manual context management",
                    SettingsAction::Compaction(true),
                    SettingsAction::Compaction(false),
                ),
            ),
            section_choice(
                "Reserve Tokens",
                tokens(host.compaction_reserve),
                None,
                None,
                "Reserve Tokens",
                numeric_options(
                    &[8_192, 16_384, 32_768],
                    host.compaction_reserve,
                    "tokens reserved for system context",
                    SettingsAction::CompactionReserve,
                ),
            ),
            section_choice(
                "Keep Recent Tokens",
                tokens(host.compaction_keep),
                None,
                None,
                "Keep Recent Tokens",
                numeric_options(
                    &[10_000, 20_000, 30_000, 50_000],
                    host.compaction_keep,
                    "recent tokens retained verbatim",
                    SettingsAction::CompactionKeep,
                ),
            ),
            section_choice(
                "Growth Guard",
                format!("{}%", host.compaction_min_growth_fraction * 100.0),
                None,
                None,
                "Growth Guard",
                [0.0625, 0.125, 0.25]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &format!("{}%", value * 100.0),
                            "Minimum context growth before compacting again",
                            SettingsAction::CompactionMinGrowthFraction(value),
                            (host.compaction_min_growth_fraction - value).abs() < f64::EPSILON,
                        )
                    })
                    .collect(),
            ),
        ],
    )
}

fn retry(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let host = &snap.host;
    section_branch(
        "API Retries",
        if host.retry_enabled {
            format!(
                "on · {} attempts · {}s budget",
                host.retry_max_retries,
                host.retry_budget_ms / 1000
            )
        } else {
            "off".into()
        },
        None,
        Some(GROUP_CONTEXT),
        vec![
            section_choice(
                "Enabled",
                on_off(host.retry_enabled).into(),
                None,
                None,
                "API Retries",
                binary_options(
                    host.retry_enabled,
                    "Retry transient model failures",
                    "Fail on the first model error",
                    SettingsAction::Retry(true),
                    SettingsAction::Retry(false),
                ),
            ),
            section_choice(
                "Maximum Retries",
                host.retry_max_retries.to_string(),
                None,
                None,
                "Maximum Retries",
                [1, 3, 5]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &value.to_string(),
                            "Maximum retry attempts per model request",
                            SettingsAction::RetryMaxRetries(value),
                            host.retry_max_retries == value,
                        )
                    })
                    .collect(),
            ),
            millis_choice(
                "Base Delay",
                host.retry_base_delay_ms,
                &[500, 2_000, 5_000],
                SettingsAction::RetryBaseDelay,
            ),
            millis_choice(
                "Maximum Delay",
                host.retry_max_delay_ms,
                &[10_000, 30_000, 60_000],
                SettingsAction::RetryMaxDelay,
            ),
            millis_choice(
                "Retry Budget",
                host.retry_budget_ms,
                &[30_000, 60_000, 120_000],
                SettingsAction::RetryBudget,
            ),
        ],
    )
}

fn millis_choice(
    title: &str,
    current: u64,
    values: &[u64],
    action: fn(u64) -> SettingsAction,
) -> MenuRow<SettingsAction> {
    section_choice(
        title,
        duration(current),
        None,
        None,
        title,
        values
            .iter()
            .map(|value| option_row(&duration(*value), title, action(*value), current == *value))
            .collect(),
    )
}

fn numeric_options(
    values: &[u64],
    current: u64,
    detail: &str,
    action: fn(u64) -> SettingsAction,
) -> Vec<MenuRow<SettingsAction>> {
    values
        .iter()
        .map(|value| option_row(&tokens(*value), detail, action(*value), current == *value))
        .collect()
}

fn duration(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

fn tokens(value: u64) -> String {
    if value >= 1024 && value.is_multiple_of(1024) {
        format!("{}k", value / 1024)
    } else if value >= 1000 && value.is_multiple_of(1000) {
        format!("{}k", value / 1000)
    } else {
        value.to_string()
    }
}
