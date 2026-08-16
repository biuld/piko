use super::*;
use crate::{
    app::{
        command::{Action, SurfaceAction},
        config_command_for_setting,
    },
    features::settings::SettingsAction,
};

fn patch(action: SettingsAction) -> serde_json::Value {
    let piko_protocol::Command::ConfigUpdate { patch, .. } = config_command_for_setting(action)
    else {
        panic!("settings action must produce ConfigUpdate")
    };
    patch
}

#[test]
fn expanded_host_actions_emit_minimal_merge_patches() {
    assert_eq!(
        patch(SettingsAction::RetryBudget(120_000)),
        json!({ "retry": { "budget-ms": 120000 } })
    );
    assert_eq!(
        patch(SettingsAction::Feature("exec", false)),
        json!({ "features": { "exec": false } })
    );
    assert_eq!(
        patch(SettingsAction::PermissionProfile("locked".into())),
        json!({ "permissions": { "profile": "locked" } })
    );
    assert_eq!(
        patch(SettingsAction::PromptCache("ephemeral")),
        json!({ "prompt": { "cache-policy": "ephemeral" } })
    );
    assert_eq!(
        patch(SettingsAction::Trajectory(true)),
        json!({ "trajectory": { "enabled": true } })
    );
}

#[test]
fn expanded_tui_actions_use_the_tui_namespace_schema() {
    assert_eq!(
        patch(SettingsAction::EditorMaxLines(10)),
        json!({ "tui": { "editor": { "maxLines": 10 } } })
    );
    assert_eq!(
        patch(SettingsAction::TreeFilter("user_only")),
        json!({ "tui": { "tree": { "filter_mode": "user_only" } } })
    );
    assert_eq!(
        patch(SettingsAction::BottomBarPreset("compact")),
        json!({ "tui": { "bottom_bar": { "items": ["agent", "model", "context"] } } })
    );
}

#[test]
fn optimistic_presentation_apply_updates_live_components() {
    let mut app = app();
    app.apply_settings_action_optimistically(&SettingsAction::EditorMaxLines(10));
    app.apply_settings_action_optimistically(&SettingsAction::TreeFilter("user_only"));
    app.apply_settings_action_optimistically(&SettingsAction::BottomBarPreset("minimal"));

    assert_eq!(app.tui_config.editor.max_lines, 10);
    assert_eq!(
        app.tui_config.tree.filter_mode,
        crate::config::TreeFilterMode::UserOnly
    );
    assert_eq!(app.tui_config.bottom_bar.items.len(), 2);
}

#[test]
fn opening_settings_refreshes_both_authoritative_namespaces() {
    let mut app = app();
    let effects = app.dispatch(Action::Surface(SurfaceAction::OpenSettings));
    let mut namespaces: Vec<String> = effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Send(piko_protocol::Command::ConfigGet { namespace, .. }) => Some(namespace),
            _ => None,
        })
        .collect();
    namespaces.sort();
    assert_eq!(namespaces, vec!["host", "tui"]);
}
