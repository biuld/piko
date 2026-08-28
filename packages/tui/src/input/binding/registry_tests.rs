use super::*;
use crate::{
    app::{AppState, InitialOptions},
    input::command::{CommandId, command_spec},
    terminal::{KeyPhase, TerminalProfile},
};

fn app() -> AppState {
    AppState::new(
        std::path::PathBuf::from("."),
        None,
        false,
        InitialOptions::default(),
    )
}

#[test]
fn baseline_profile_selects_ctrl_j_for_newline() {
    let registry = BindingRegistry::compile(TerminalProfile::baseline(), None).unwrap();
    let app = app();
    let context = BindingContext::from_app(&app, registry.profile());
    let scopes = active_scope_stack(&app);
    assert_eq!(
        registry.binding_for(CommandId::EditorNewline, &context, &scopes),
        Some(KeyStroke::parse("ctrl+j").unwrap())
    );
}

#[test]
fn enhanced_profile_selects_shift_enter_for_newline() {
    let registry = BindingRegistry::default();
    let app = app();
    let context = BindingContext::from_app(&app, registry.profile());
    let scopes = active_scope_stack(&app);
    assert_eq!(
        registry.binding_for(CommandId::EditorNewline, &context, &scopes),
        Some(KeyStroke::parse("shift+enter").unwrap())
    );
}

#[test]
fn disabled_builtin_is_removed_by_rule_id() {
    let mut settings = KeybindingSettings::default();
    settings.rules.insert(
        "default-app-quit".to_string(),
        BindingRuleSetting {
            enabled: Some(false),
            ..Default::default()
        },
    );
    let registry = BindingRegistry::compile(TerminalProfile::baseline(), Some(&settings)).unwrap();
    assert!(
        registry
            .rules()
            .iter()
            .find(|rule| rule.id == "default-app-quit")
            .is_some_and(|rule| !rule.enabled)
    );
    assert!(
        registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("disabled"))
    );
}

#[test]
fn conflicting_custom_rules_are_reported() {
    let mut settings = KeybindingSettings::default();
    settings.rules.insert(
        "custom-one".to_string(),
        BindingRuleSetting {
            key: Some("ctrl+x".to_string()),
            command: Some("editor.submit".to_string()),
            scope: Some("editor".to_string()),
            ..Default::default()
        },
    );
    settings.rules.insert(
        "custom-two".to_string(),
        BindingRuleSetting {
            key: Some("ctrl+x".to_string()),
            command: Some("editor.newline".to_string()),
            scope: Some("editor".to_string()),
            when: Some(vec!["editor.multiline".to_string()]),
            ..Default::default()
        },
    );
    let errors = BindingRegistry::compile(TerminalProfile::baseline(), Some(&settings))
        .expect_err("conflicting rules must fail compilation");
    assert!(
        errors
            .iter()
            .any(|diagnostic| diagnostic.message.contains("conflicting rules"))
    );
}

#[test]
fn unreachable_shift_enter_is_reported_without_disabling_the_registry() {
    let mut settings = KeybindingSettings::default();
    settings.rules.insert(
        "default-editor-newline-enhanced".to_string(),
        BindingRuleSetting {
            key: Some("shift+enter".to_string()),
            when: Some(vec!["editor.multiline".to_string()]),
            ..Default::default()
        },
    );
    let registry = BindingRegistry::compile(TerminalProfile::baseline(), Some(&settings)).unwrap();
    assert!(registry.diagnostics().iter().any(|diagnostic| {
        diagnostic.rule_id.as_deref() == Some("default-editor-newline-enhanced")
            && diagnostic.message.contains("unreachable")
    }));
}

#[test]
fn press_only_commands_ignore_repeat_events() {
    let registry = BindingRegistry::default();
    let app = app();
    let context = BindingContext::from_app(&app, registry.profile());
    let scopes = active_scope_stack(&app);
    assert_eq!(
        registry.resolve(
            KeyStroke::parse("ctrl+d").unwrap(),
            KeyPhase::Repeat,
            &context,
            &scopes,
        ),
        Resolution::Unhandled
    );
}

#[test]
fn workflow_backspace_requires_an_active_text_input() {
    let registry = BindingRegistry::default();
    let app = app();
    let mut context = BindingContext::from_app(&app, registry.profile());
    let scopes = ScopeStack::new(vec![ActiveScope::blocking(
        ScopeKind::ToolInteraction,
        Some(TextSink::Surface),
    )]);
    let key = KeyStroke::parse("backspace").unwrap();

    assert_eq!(
        registry.resolve(key, KeyPhase::Press, &context, &scopes),
        Resolution::Consumed
    );
    context.text_input_active = true;
    assert_eq!(
        registry.resolve(key, KeyPhase::Press, &context, &scopes),
        Resolution::Command {
            command: CommandId::TextDeleteBackward,
            rule_id: "default-workflow-delete-backward".to_string(),
        }
    );
}

#[test]
fn every_default_rule_targets_a_catalogued_scope() {
    for rule in super::defaults::default_rules() {
        let spec = command_spec(rule.command).expect("default command is catalogued");
        assert!(spec.scopes.contains(&rule.scope), "{}", rule.id);
    }
}
