use std::fs;

use hostd::domain::config::{CompactionSettings, HostSettings, SettingsManager};

#[test]
fn in_memory_settings_apply_defaults_and_overrides() {
    let manager = SettingsManager::in_memory(HostSettings {
        default_model: Some("gpt-test".into()),
        theme: Some("dark".into()),
        ..HostSettings::default()
    });

    assert_eq!(manager.get_default_model(), Some("gpt-test"));
    assert_eq!(manager.get_theme(), Some("dark"));
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
theme = "global-theme"

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
    assert_eq!(manager.get_theme(), Some("global-theme"));
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
