//! Integration smoke against a real `piko-hostd` binary (no GPUI window).
//!
//! Skips when the hostd binary cannot be resolved on disk.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use piko_client_core::{ClientIntent, SessionPhase};
use piko_protocol::SessionListScope;

use crate::bridge::spawn_bridge;
use crate::transport::discovery::resolve_hostd_command;

fn hostd_available() -> bool {
    let cmd = resolve_hostd_command();
    Path::new(&cmd).is_file()
}

fn poll_until(
    bridge: &mut crate::bridge::ClientBridge,
    timeout: Duration,
    mut pred: impl FnMut(&crate::bridge::ClientBridge) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        bridge.poll();
        if pred(bridge) {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    bridge.poll();
    pred(bridge)
}

fn pending_cleared(
    bridge: &crate::bridge::ClientBridge,
    match_op: impl Fn(&piko_client_core::state::PendingOp) -> bool,
) -> bool {
    !bridge.state().pending_commands.values().any(match_op)
}

#[test]
fn hostd_smoke_discover_create_reconcile_list_models() {
    if !hostd_available() {
        eprintln!(
            "skip hostd smoke: binary not found ({})",
            resolve_hostd_command()
        );
        return;
    }

    let cwd = std::env::temp_dir()
        .join(format!("piko-gui-smoke-{}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    std::fs::create_dir_all(&cwd).expect("temp cwd");
    let session_root = std::env::temp_dir()
        .join(format!("piko-gui-smoke-sessions-{}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    let mut bridge = spawn_bridge(
        &[],
        &[
            ("PIKO_LOG_DISABLE", "1"),
            ("PIKO_SESSION_DIR", session_root.as_str()),
        ],
    )
    .expect("spawn hostd");

    bridge.intent(ClientIntent::DiscoverSessions {
        scope: SessionListScope::CurrentFolder,
        cwd: Some(cwd.clone()),
    });
    assert!(
        poll_until(&mut bridge, Duration::from_secs(5), |b| {
            pending_cleared(b, |op| {
                matches!(op, piko_client_core::state::PendingOp::Discover)
            })
        }),
        "SessionList did not complete: err={:?}",
        bridge.state().last_error
    );

    bridge.intent(ClientIntent::CreateSession { cwd });
    assert!(
        poll_until(&mut bridge, Duration::from_secs(10), |b| {
            b.state().session_phase == SessionPhase::Live && b.state().live_session.is_some()
        }),
        "CreateSession did not reach Live: phase={:?} err={:?}",
        bridge.state().session_phase,
        bridge.state().last_error
    );

    let session_id = bridge
        .state()
        .live_session
        .as_ref()
        .map(|s| s.session_id.clone())
        .expect("live session id");
    assert!(!session_id.is_empty());

    bridge.intent(ClientIntent::OpenSession {
        session_id: session_id.clone(),
        session_path: None,
    });
    assert!(
        poll_until(&mut bridge, Duration::from_secs(10), |b| {
            b.state().session_phase == SessionPhase::Live
                && b.state()
                    .live_session
                    .as_ref()
                    .is_some_and(|session| session.session_id == session_id)
        }),
        "OpenSession did not return to Live: phase={:?} err={:?}",
        bridge.state().session_phase,
        bridge.state().last_error
    );

    bridge.intent(ClientIntent::ListModels);
    assert!(
        poll_until(&mut bridge, Duration::from_secs(5), |b| {
            pending_cleared(b, |op| {
                matches!(op, piko_client_core::state::PendingOp::ListModels)
            })
        }),
        "ModelList did not complete: err={:?}",
        bridge.state().last_error
    );

    let has_model = bridge
        .state()
        .model
        .providers
        .iter()
        .any(|p| !p.models.is_empty());
    if !has_model {
        eprintln!("skip submit/cancel: no models in catalog (auth may be missing)");
        bridge.shutdown();
        return;
    }

    // Prefer selecting a concrete model when catalog is non-empty.
    if let Some(provider) = bridge
        .state()
        .model
        .providers
        .iter()
        .find(|p| !p.models.is_empty())
    {
        let model_id = provider.models[0].id.clone();
        let provider_id = provider.provider.clone();
        bridge.intent(ClientIntent::SetModel {
            provider: provider_id,
            model_id,
        });
        thread::sleep(Duration::from_millis(200));
        bridge.poll();
    }

    bridge.intent(ClientIntent::SubmitTurn {
        text: "ping".into(),
    });
    assert!(
        poll_until(&mut bridge, Duration::from_secs(15), |b| {
            pending_cleared(b, |op| {
                matches!(op, piko_client_core::state::PendingOp::Submit)
            }) || b.state().last_error.is_some()
        }),
        "SubmitTurn did not settle: err={:?}",
        bridge.state().last_error
    );

    // Cancel if a turn became active; otherwise the submit may have failed fast.
    if bridge
        .state()
        .live_session
        .as_ref()
        .is_some_and(|s| !s.active_turns.is_empty())
    {
        bridge.intent(ClientIntent::CancelTurn);
        assert!(
            poll_until(&mut bridge, Duration::from_secs(10), |b| {
                pending_cleared(b, |op| {
                    matches!(op, piko_client_core::state::PendingOp::Cancel)
                }) || b
                    .state()
                    .live_session
                    .as_ref()
                    .is_some_and(|s| s.active_turns.is_empty())
            }),
            "CancelTurn did not settle: err={:?}",
            bridge.state().last_error
        );
    }

    assert_eq!(bridge.state().session_phase, SessionPhase::Live);
    bridge.shutdown();
}
