use std::fs;

use piko_hostd::domain::config::{CompactionSettings, HostSettings, SettingsManager};

#[test]
fn in_memory_settings_apply_defaults_and_overrides() {
    let manager = SettingsManager::in_memory(HostSettings {
        default_model: Some("gpt-test".into()),
        ..HostSettings::default()
    });

    assert_eq!(manager.get_default_model(), Some("gpt-test"));
    assert_eq!(manager.get_transport(), "auto");
    assert_eq!(manager.get_compaction_settings(), (true, 16384, 20000));
}

#[test]
fn project_settings_override_global_settings_and_preserve_nested_defaults() {
    let temp_global = tempfile::tempdir().unwrap();
    let temp_project = tempfile::tempdir().unwrap();

    let global_dir = temp_global.path();
    fs::write(
        global_dir.join("settings.toml"),
        r#"
default-model = "global-model"

[compaction]
reserve-tokens = 111
"#,
    )
    .unwrap();

    let project_dir = temp_project.path().join(".piko");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        project_dir.join("settings.toml"),
        r#"
default-model = "project-model"

[compaction]
keep-recent-tokens = 222
"#,
    )
    .unwrap();

    let manager = SettingsManager::from_paths(
        global_dir.join("settings.toml"),
        project_dir.join("settings.toml"),
        HostSettings::default(),
    )
    .unwrap();

    assert_eq!(manager.get_default_model(), Some("project-model"));
    assert_eq!(manager.get_compaction_settings(), (true, 111, 222));
}

#[test]
fn apply_overrides_merges_nested_settings() {
    let mut manager = SettingsManager::in_memory(HostSettings {
        compaction: Some(CompactionSettings {
            enabled: Some(false),
            reserve_tokens: Some(100),
            keep_recent_tokens: None,
        }),
        ..HostSettings::default()
    });

    manager.apply_overrides(HostSettings {
        compaction: Some(CompactionSettings {
            enabled: None,
            reserve_tokens: None,
            keep_recent_tokens: Some(300),
        }),
        ..HostSettings::default()
    });

    assert_eq!(manager.get_compaction_settings(), (false, 100, 300));
}

#[test]
fn gui_namespace_is_opaque_and_project_overridable() {
    let mut manager = SettingsManager::in_memory(HostSettings {
        gui: Some(serde_json::json!({"session-open": true})),
        ..HostSettings::default()
    });
    assert_eq!(manager.settings().gui.unwrap()["session-open"], true);

    manager.apply_overrides(HostSettings {
        gui: Some(serde_json::json!({"session-open": false})),
        ..HostSettings::default()
    });
    assert_eq!(manager.settings().gui.unwrap()["session-open"], false);
}

#[test]
fn namespace_value_returns_frontend_blobs_as_stored() {
    let settings = HostSettings {
        tui: Some(serde_json::json!({
            "theme": {"name": "dark"},
            "hide_thinking_block": false,
        })),
        gui: Some(serde_json::json!({
            "hide-thinking-block": true,
        })),
        ..HostSettings::default()
    };

    let tui_value = settings.namespace_value("tui");
    assert_eq!(tui_value["theme"]["name"], "dark");
    assert_eq!(tui_value["hide_thinking_block"], false);

    let gui_value = settings.namespace_value("gui");
    assert_eq!(gui_value["hide-thinking-block"], true);
}

#[test]
fn host_namespace_excludes_frontend_blobs() {
    let settings = HostSettings {
        default_provider: Some("openai".into()),
        default_model: Some("gpt-test".into()),
        compaction: Some(CompactionSettings {
            enabled: Some(false),
            reserve_tokens: None,
            keep_recent_tokens: None,
        }),
        tui: Some(serde_json::json!({ "theme": { "name": "dark" } })),
        gui: Some(serde_json::json!({ "reduced-motion": true })),
        ..HostSettings::default()
    };

    let host_value = settings.namespace_value("host");
    assert_eq!(host_value["default-provider"], "openai");
    assert_eq!(host_value["default-model"], "gpt-test");
    assert!(host_value.get("tui").is_none());
    assert!(host_value.get("gui").is_none());
    assert!(host_value.get("theme").is_none());
    assert_eq!(host_value["compaction"]["enabled"], false);
}

#[test]
fn unknown_top_level_keys_are_ignored_on_load() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    fs::write(
        &path,
        r#"
default-model = "kept"
theme = "ignored-legacy"
hide-thinking-block = true
"#,
    )
    .unwrap();

    let manager = SettingsManager::from_paths(
        &path,
        temp.path().join("missing.toml"),
        HostSettings::default(),
    )
    .unwrap();

    assert_eq!(manager.get_default_model(), Some("kept"));
    assert!(manager.settings().tui.is_none());
    assert!(manager.settings().gui.is_none());
}
