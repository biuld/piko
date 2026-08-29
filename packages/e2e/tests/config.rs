#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult, ModelEvent, ServerMessage};
use support::{HostdHarness, serial_guard};

#[test]
fn config_update_emits_model_event_and_preserves_command_correlation() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");

    host.send(Command::ConfigGet {
        command_id: "get-host".into(),
        namespace: "host".into(),
    });
    assert!(matches!(
        host.command_result("get-host"),
        CommandResult::ConfigEntry { namespace, value }
            if namespace == "host" && value.is_object()
    ));

    host.send(Command::ConfigUpdate {
        command_id: "update-tui".into(),
        patch: serde_json::json!({
            "tui": {"bottom_bar": {"items": ["e2e"]}}
        }),
    });
    let result = host.command_result("update-tui");
    assert!(matches!(
        result,
        CommandResult::ConfigEntry { namespace, value }
            if namespace == "tui" && value["bottom_bar"]["items"][0] == "e2e"
    ));

    assert!(matches!(
        host.wait_for("model config event", |message| {
            matches!(
                message,
                ServerMessage::Model(ModelEvent::ConfigChanged {
                    model_id,
                    provider,
                    ..
                }) if model_id.is_empty() && provider.is_empty()
            )
        }),
        ServerMessage::Model(_)
    ));

    host.send(Command::ConfigGet {
        command_id: "get-tui".into(),
        namespace: "tui".into(),
    });
    assert!(matches!(
        host.command_result("get-tui"),
        CommandResult::ConfigEntry { value, .. }
            if value["bottom_bar"]["items"][0] == "e2e"
    ));
}

#[test]
fn invalid_config_patch_is_rejected_over_jsonl_without_mutating_state() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");

    host.send(Command::ConfigUpdate {
        command_id: "invalid".into(),
        patch: serde_json::json!({"default-model": 42}),
    });
    assert!(
        host.command_error("invalid")
            .contains("Invalid config patch")
    );

    host.send(Command::ConfigGet {
        command_id: "get-host".into(),
        namespace: "host".into(),
    });
    let result = host.command_result("get-host");
    assert!(matches!(
        result,
        CommandResult::ConfigEntry { value, .. }
            if value["default-model"].is_null()
    ));
}
