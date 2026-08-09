use super::*;
use crate::config::bottom_bar::BottomBarItem;

const OTEL_PRESETS: &[&str] = &["http://127.0.0.1:4318", "http://localhost:4318"];

pub(super) fn prompt_cache(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let current = snap.host.prompt_cache_policy.as_str();
    section_choice(
        "Prompt Cache",
        current.to_string(),
        None,
        Some(GROUP_MODEL),
        "Prompt Cache",
        [
            ("disabled", "Do not request provider prompt caching"),
            ("provider-default", "Use provider default cache behavior"),
            ("ephemeral", "Request ephemeral provider caching"),
            ("extended", "Request extended provider caching"),
        ]
        .into_iter()
        .map(|(value, detail)| {
            option_row(
                value,
                detail,
                SettingsAction::PromptCache(value),
                current == value,
            )
        })
        .collect(),
    )
}

pub(super) fn otel_endpoint(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let current = &snap.host.otel_endpoint;
    let summary = if OTEL_PRESETS.contains(&current.as_str()) {
        current.clone()
    } else {
        format!("{current} · custom")
    };
    section_choice(
        "OTLP Endpoint",
        summary,
        Some("restart hostd"),
        None,
        "OTLP Endpoint",
        vec![
            option_row(
                "Local collector",
                OTEL_PRESETS[0],
                SettingsAction::ObservabilityEndpoint(OTEL_PRESETS[0]),
                current == OTEL_PRESETS[0],
            ),
            option_row(
                "Localhost",
                OTEL_PRESETS[1],
                SettingsAction::ObservabilityEndpoint(OTEL_PRESETS[1]),
                current == OTEL_PRESETS[1],
            ),
        ],
    )
}

pub(super) fn rows(snap: &SettingsSnapshot) -> Vec<MenuRow<SettingsAction>> {
    vec![
        section_choice(
            "Thinking Blocks",
            if snap.thinking_visible {
                "shown".into()
            } else {
                "hidden".into()
            },
            None,
            Some(GROUP_APPEARANCE),
            "Thinking Blocks",
            vec![
                option_row(
                    "Shown",
                    "Render reasoning blocks",
                    SettingsAction::HideThinking(false),
                    snap.thinking_visible,
                ),
                option_row(
                    "Hidden",
                    "Hide reasoning blocks",
                    SettingsAction::HideThinking(true),
                    !snap.thinking_visible,
                ),
            ],
        ),
        theme(snap),
        editor(snap),
        tree_filter(snap),
        bottom_bar(snap),
    ]
}

fn theme(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
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
    )
}

fn editor(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let editor = &snap.tui.editor;
    section_branch(
        "Editor",
        format!(
            "{} lines · history {}",
            editor.max_lines, editor.history_limit
        ),
        None,
        Some(GROUP_APPEARANCE),
        vec![
            section_choice(
                "Multiline",
                on_off(editor.multiline).into(),
                None,
                None,
                "Editor Multiline",
                binary_options(
                    editor.multiline,
                    "Allow multiline input",
                    "Single-line input",
                    SettingsAction::EditorMultiline(true),
                    SettingsAction::EditorMultiline(false),
                ),
            ),
            section_choice(
                "Auto Resize",
                on_off(editor.auto_resize).into(),
                None,
                None,
                "Editor Auto Resize",
                binary_options(
                    editor.auto_resize,
                    "Grow with content",
                    "Keep a fixed editor height",
                    SettingsAction::EditorAutoResize(true),
                    SettingsAction::EditorAutoResize(false),
                ),
            ),
            section_choice(
                "Maximum Lines",
                editor.max_lines.to_string(),
                None,
                None,
                "Editor Maximum Lines",
                [3, 6, 10, 16]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &value.to_string(),
                            "Maximum visible composer lines",
                            SettingsAction::EditorMaxLines(value),
                            editor.max_lines == value,
                        )
                    })
                    .collect(),
            ),
            section_choice(
                "History Limit",
                editor.history_limit.to_string(),
                None,
                None,
                "Editor History Limit",
                [50, 100, 250, 500]
                    .into_iter()
                    .map(|value| {
                        option_row(
                            &value.to_string(),
                            "Retained local input history entries",
                            SettingsAction::EditorHistoryLimit(value),
                            editor.history_limit == value,
                        )
                    })
                    .collect(),
            ),
        ],
    )
}

fn tree_filter(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let current = tree_filter_name(snap.tui.tree.filter_mode);
    section_choice(
        "Tree Filter",
        current.replace('_', " "),
        None,
        Some(GROUP_APPEARANCE),
        "Tree Filter",
        ["default", "no_tools", "user_only", "labeled_only", "all"]
            .into_iter()
            .map(|value| {
                option_row(
                    &value.replace('_', " "),
                    "Default filter when Tree opens",
                    SettingsAction::TreeFilter(value),
                    current == value,
                )
            })
            .collect(),
    )
}

fn bottom_bar(snap: &SettingsSnapshot) -> MenuRow<SettingsAction> {
    let current = bottom_bar_preset(&snap.tui.bottom_bar.items);
    section_choice(
        "Bottom Bar",
        current.unwrap_or("custom").into(),
        None,
        Some(GROUP_APPEARANCE),
        "Bottom Bar",
        [
            ("full", "Agent, model, cwd, context, and cost"),
            ("compact", "Agent, model, and context"),
            ("minimal", "Agent and model"),
        ]
        .into_iter()
        .map(|(value, detail)| {
            option_row(
                value,
                detail,
                SettingsAction::BottomBarPreset(value),
                current == Some(value),
            )
        })
        .collect(),
    )
}

fn tree_filter_name(mode: crate::config::TreeFilterMode) -> &'static str {
    match mode {
        crate::config::TreeFilterMode::Default => "default",
        crate::config::TreeFilterMode::NoTools => "no_tools",
        crate::config::TreeFilterMode::UserOnly => "user_only",
        crate::config::TreeFilterMode::LabeledOnly => "labeled_only",
        crate::config::TreeFilterMode::All => "all",
    }
}

fn bottom_bar_preset(items: &[BottomBarItem]) -> Option<&'static str> {
    use BottomBarItem::*;
    match items {
        [Agent, Model, Cwd, Context, Cost] => Some("full"),
        [Agent, Model, Context] => Some("compact"),
        [Agent, Model] => Some("minimal"),
        _ => None,
    }
}
