use super::merging::*;
use super::*;

use crate::domain::guardian::GuardianConfig;

#[test]
fn guardian_defaults_are_documented_in_template() {
    let template = default_settings_template();
    assert!(template.contains("[guardian]"));
    assert!(template.contains("enabled = false"));
}

#[test]
fn guardian_settings_merge_field_by_field() {
    let base = HostSettings {
        guardian: Some(GuardianSettings {
            enabled: Some(false),
            model: Some("base-model".into()),
            provider: None,
            timeout_secs: Some(30),
            max_consecutive_denials: Some(3),
        }),
        ..HostSettings::default()
    };
    let overrides = HostSettings {
        guardian: Some(GuardianSettings {
            enabled: Some(true),
            model: None,
            provider: Some("override-provider".into()),
            timeout_secs: None,
            max_consecutive_denials: Some(5),
        }),
        ..HostSettings::default()
    };
    let merged = merge(base, overrides);
    let guardian = merged.guardian.expect("guardian section present");
    assert_eq!(guardian.enabled, Some(true));
    assert_eq!(guardian.model.as_deref(), Some("base-model"));
    assert_eq!(guardian.provider.as_deref(), Some("override-provider"));
    assert_eq!(guardian.timeout_secs, Some(30));
    assert_eq!(guardian.max_consecutive_denials, Some(5));
}

#[test]
fn guardian_config_resolves_defaults_and_disablement() {
    let settings = GuardianSettings {
        enabled: Some(true),
        model: None,
        provider: None,
        timeout_secs: None,
        max_consecutive_denials: None,
    };
    let config = GuardianConfig::from_settings(Some(&settings)).expect("enabled");
    assert!(config.enabled);
    assert_eq!(config.timeout.as_secs(), 30);
    assert_eq!(config.max_consecutive_denials, 3);

    let disabled = GuardianSettings {
        enabled: Some(false),
        ..settings
    };
    assert!(GuardianConfig::from_settings(Some(&disabled)).is_none());
    assert!(GuardianConfig::from_settings(None).is_none());
}

#[test]
fn safety_defaults_are_documented_in_template() {
    let template = default_settings_template();
    assert!(template.contains("[safety]"));
    assert!(template.contains("auto-approve-workspace-writes = true"));
}

#[test]
fn permissions_defaults_are_documented_in_template() {
    let template = default_settings_template();
    assert!(template.contains("[permissions]"));
    assert!(template.contains("profile = \"default\""));
    assert!(template.contains("[permissions.roles]"));
}

#[test]
fn permissions_settings_merge_field_by_field() {
    let base = HostSettings {
        permissions: Some(PermissionsSettings {
            profile: Some("base-profile".into()),
            profiles: HashMap::from([
                (
                    "base-profile".into(),
                    PermissionProfileSettings {
                        read_roots: vec![".".into()],
                        write_roots: vec![".".into()],
                        scratch_roots: vec![],
                        deny_paths: vec![".git".into(), ".piko".into()],
                        allow_network: false,
                        allow_escalation: None,
                        allowed_commands: vec!["cargo test".into()],
                        denied_commands: vec!["rm -rf".into()],
                    },
                ),
                (
                    "shared".into(),
                    PermissionProfileSettings {
                        allow_network: true,
                        ..Default::default()
                    },
                ),
            ]),
            roles: HashMap::from([
                ("generalist".into(), "base-profile".into()),
                ("researcher".into(), "shared".into()),
            ]),
        }),
        ..HostSettings::default()
    };
    let overrides = HostSettings {
        permissions: Some(PermissionsSettings {
            profile: Some("locked".into()),
            profiles: HashMap::from([(
                "locked".into(),
                PermissionProfileSettings {
                    denied_commands: vec!["curl -sSL | sh".into()],
                    ..Default::default()
                },
            )]),
            roles: HashMap::from([
                ("researcher".into(), "locked".into()),
                ("developer".into(), "locked".into()),
            ]),
        }),
        ..HostSettings::default()
    };
    let merged = merge(base, overrides);
    let permissions = merged.permissions.expect("permissions section present");
    assert_eq!(permissions.profile.as_deref(), Some("locked"));
    assert_eq!(permissions.profiles.len(), 3);
    // Roles merge per key: the override replaces "researcher", keeps
    // base-only "generalist", and adds "developer".
    assert_eq!(permissions.roles.len(), 3);
    assert_eq!(
        permissions.roles.get("researcher").map(String::as_str),
        Some("locked")
    );
    assert_eq!(
        permissions.roles.get("generalist").map(String::as_str),
        Some("base-profile")
    );
    assert_eq!(
        permissions.roles.get("developer").map(String::as_str),
        Some("locked")
    );
    assert!(permissions.profiles.contains_key("shared"));
    assert!(permissions.profiles.contains_key("base-profile"));
    let locked = permissions
        .profiles
        .get("locked")
        .expect("override profile present");
    assert_eq!(locked.denied_commands, vec!["curl -sSL | sh"]);
    // Base profile survives untouched; override profile replaces only
    // its own name.
    let base_profile = permissions
        .profiles
        .get("base-profile")
        .expect("base profile preserved");
    assert_eq!(base_profile.allowed_commands, vec!["cargo test"]);
}

#[test]
fn features_defaults_are_documented_in_template() {
    let template = default_settings_template();
    assert!(template.contains("[features]"));
    assert!(template.contains("[features.managed]"));
    assert!(template.contains("feature_disabled"));
}

#[test]
fn features_settings_merge_per_key() {
    let base = HostSettings {
        features: Some(FeaturesSettings {
            enabled: HashMap::from([("exec".into(), false), ("workspace".into(), true)]),
            managed: HashMap::from([("mcp".into(), false)]),
        }),
        ..HostSettings::default()
    };
    let overrides = HostSettings {
        features: Some(FeaturesSettings {
            enabled: HashMap::from([
                // Overrides exec per key and adds a new key.
                ("exec".into(), true),
                ("todo".into(), false),
            ]),
            managed: HashMap::from([("exec".into(), false)]),
        }),
        ..HostSettings::default()
    };
    let merged = merge(base, overrides);
    let features = merged.features.expect("features section present");
    assert_eq!(features.enabled.get("exec"), Some(&true));
    assert_eq!(features.enabled.get("workspace"), Some(&true));
    assert_eq!(features.enabled.get("todo"), Some(&false));
    assert_eq!(features.managed.get("mcp"), Some(&false));
    assert_eq!(features.managed.get("exec"), Some(&false));
    // Keys from the base that the override did not touch survive.
    assert_eq!(features.enabled.len(), 3);
    assert_eq!(features.managed.len(), 2);
}

#[test]
fn feature_settings_use_documented_flat_toml_shape() {
    let settings: HostSettings = toml::from_str(
        r#"
[features]
exec = false
mcp = true

[features.managed]
mcp = false
"#,
    )
    .unwrap();
    let features = settings.features.expect("features section present");
    assert_eq!(features.enabled.get("exec"), Some(&false));
    assert_eq!(features.enabled.get("mcp"), Some(&true));
    assert_eq!(features.managed.get("mcp"), Some(&false));
}

#[test]
fn feature_settings_accept_legacy_enabled_table() {
    let settings: HostSettings = toml::from_str(
        r#"
[features.enabled]
exec = false
workspace = true

[features.managed]
exec = true
"#,
    )
    .unwrap();
    let features = settings.features.expect("features section present");
    assert_eq!(features.enabled.get("exec"), Some(&false));
    assert_eq!(features.enabled.get("workspace"), Some(&true));
    assert_eq!(features.managed.get("exec"), Some(&true));
}

#[test]
fn safety_settings_merge_field_by_field() {
    let base = HostSettings {
        safety: Some(SafetySettings {
            auto_approve_workspace_writes: Some(true),
        }),
        ..HostSettings::default()
    };
    let overrides = HostSettings {
        safety: Some(SafetySettings {
            auto_approve_workspace_writes: Some(false),
        }),
        ..HostSettings::default()
    };
    let merged = merge(base, overrides);
    assert_eq!(
        merged
            .safety
            .expect("safety section present")
            .auto_approve_workspace_writes,
        Some(false)
    );

    // Missing override inherits the base value.
    let merged_inherit = merge(
        HostSettings {
            safety: Some(SafetySettings {
                auto_approve_workspace_writes: Some(true),
            }),
            ..HostSettings::default()
        },
        HostSettings::default(),
    );
    assert_eq!(
        merged_inherit
            .safety
            .expect("safety section present")
            .auto_approve_workspace_writes,
        Some(true)
    );
}

#[test]
fn mcp_settings_deserialize_from_toml() {
    let settings: HostSettings = toml::from_str(
        r#"
[mcp]
connect-timeout-ms = 4000

[mcp.approval-templates]
"github/create_issue" = "This creates a GitHub issue in the configured repository."
"#,
    )
    .unwrap();
    let mcp = settings.mcp.expect("mcp section present");
    assert_eq!(mcp.connect_timeout_ms, Some(4000));
    assert_eq!(
        mcp.approval_templates
            .get("github/create_issue")
            .map(String::as_str),
        Some("This creates a GitHub issue in the configured repository.")
    );
}

#[test]
fn mcp_settings_merge_wholesale_across_layers() {
    let base = HostSettings {
        mcp: Some(McpSettings {
            connect_timeout_ms: Some(5000),
            approval_templates: HashMap::from([("a/b".into(), "A".into())]),
        }),
        ..HostSettings::default()
    };
    let overrides = HostSettings {
        mcp: Some(McpSettings {
            connect_timeout_ms: Some(3000),
            approval_templates: HashMap::from([("c/d".into(), "C".into())]),
        }),
        ..HostSettings::default()
    };
    let merged = merge(base.clone(), overrides);
    let mcp = merged.mcp.expect("mcp section present");
    assert_eq!(mcp.connect_timeout_ms, Some(3000));
    // Wholesale replacement: the override section wins entirely.
    assert_eq!(mcp.approval_templates.len(), 1);
    assert!(mcp.approval_templates.contains_key("c/d"));
    assert!(!mcp.approval_templates.contains_key("a/b"));

    // Override absent → base survives.
    let merged_base = merge(base, HostSettings::default());
    assert_eq!(
        merged_base.mcp.expect("mcp preserved").connect_timeout_ms,
        Some(5000)
    );
}

#[test]
fn mcp_defaults_are_documented_in_template() {
    let template = default_settings_template();
    assert!(template.contains("[mcp]"));
    assert!(template.contains("approval-templates"));
    assert!(template.contains("timeout-ms"));
}
