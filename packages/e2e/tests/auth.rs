#[path = "support/mod.rs"]
mod support;

use piko_protocol::{AuthEvent, Command, CommandResult, ServerMessage};
use support::{HostdHarness, serial_guard};

#[test]
fn api_key_auth_round_trips_through_hostd_and_is_isolated_to_the_test_home() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");

    host.send(Command::AuthSetApiKey {
        command_id: "set-key".into(),
        provider: "openai".into(),
        api_key: "e2e-secret-key".into(),
    });
    let result = host.command_result("set-key");
    assert!(matches!(
        result,
        CommandResult::ModelListed { providers, .. }
            if providers.iter().any(|provider| {
                provider.provider == "openai" && provider.has_auth
            })
    ));
    assert!(matches!(
        host.wait_for("login success", |message| {
            matches!(
                message,
                ServerMessage::Auth(AuthEvent::LoginSuccess {
                    provider, login_id: None
                }) if provider == "openai"
            )
        }),
        ServerMessage::Auth(_)
    ));

    let auth_path = host
        .workspace()
        .parent()
        .expect("e2e root")
        .join("piko-home/auth.json");
    let auth_file = std::fs::read_to_string(&auth_path).expect("isolated auth file");
    assert!(auth_file.contains("e2e-secret-key"));

    host.send(Command::AuthLogout {
        command_id: "logout".into(),
        provider: "openai".into(),
    });
    let result = host.command_result("logout");
    assert!(matches!(
        result,
        CommandResult::ModelListed { providers, .. }
            if providers.iter().any(|provider| {
                provider.provider == "openai" && !provider.has_auth
            })
    ));
    assert!(matches!(
        host.wait_for("logged out", |message| {
            matches!(
                message,
                ServerMessage::Auth(AuthEvent::LoggedOut { provider })
                    if provider == "openai"
            )
        }),
        ServerMessage::Auth(_)
    ));
    let auth_file = std::fs::read_to_string(&auth_path).expect("auth file after logout");
    assert!(!auth_file.contains("e2e-secret-key"));
}

#[test]
fn unsupported_oauth_and_duplicate_cancel_return_correlated_jsonl_errors() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");

    host.send(Command::AuthLoginOAuth {
        command_id: "oauth".into(),
        provider: "missing-provider".into(),
        mode: piko_protocol::OAuthLoginMode::DeviceCode,
    });
    let oauth_error = host.command_error("oauth");
    assert!(oauth_error.contains("OAuth not supported for provider missing-provider"));

    host.send(Command::AuthCancelOAuth {
        command_id: "cancel-oauth".into(),
        provider: "missing-provider".into(),
    });
    let cancel_error = host.command_error("cancel-oauth");
    assert!(cancel_error.contains("No active OAuth login for provider missing-provider"));
}
