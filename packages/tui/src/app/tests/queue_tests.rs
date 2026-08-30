use super::*;
use crate::app::command::EditorAction;

fn running_app() -> AppState {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("task-1".into());
    app.session.agent_work.insert(
        "task-1".into(),
        work_snapshot(Some("input-live"), Vec::new()),
    );
    app
}

fn work_snapshot(
    active_root: Option<&str>,
    queued_inputs: Vec<piko_protocol::AgentInputSummary>,
) -> piko_protocol::AgentWorkSnapshot {
    piko_protocol::AgentWorkSnapshot {
        agent_instance_id: "task-1".into(),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        foreground: if active_root.is_some() {
            piko_protocol::AgentForeground::Running
        } else if queued_inputs.is_empty() {
            piko_protocol::AgentForeground::Idle
        } else {
            piko_protocol::AgentForeground::Queued
        },
        active_work: active_root.map(|root_input_id| piko_protocol::ActiveWorkSnapshot {
            root_input_id: root_input_id.into(),
            state: piko_protocol::AgentWorkViewState::Running,
            active_model_step_id: None,
            started_at: 1,
        }),
        pending_steers: Vec::new(),
        queued_inputs,
        pending_action: None,
    }
}

fn queued_input(input_id: &str, preview: &str) -> piko_protocol::AgentInputSummary {
    piko_protocol::AgentInputSummary {
        input_id: input_id.into(),
        origin: piko_protocol::AgentInputOrigin::User,
        preview: preview.into(),
        admission_revision: 2,
        submitted_at: 2,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
    }
}

#[test]
fn interrupt_targets_viewed_agent_without_requiring_a_turn() {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("agent-child".into());

    let effects = app.dispatch(EditorAction::Interrupt.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInterrupt {
            session_id,
            agent_instance_id,
            ..
        })] if session_id == "session-1" && agent_instance_id == "agent-child"
    ));
}

#[test]
fn detached_runtime_activity_does_not_masquerade_as_a_host_turn_for_steer() {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("agent-child".into());
    app.agent_panel
        .upsert_agent(crate::features::agent_status::AgentEntry {
            agent_id: "worker".into(),
            agent_instance_id: "agent-child".into(),
            name: "worker".into(),
            parent_agent_instance_id: Some("agent-root".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Running,
        });
    app.editor.restore_text("next task");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInputSubmit {
            input,
            ..
        })] if input.agent_instance_id == "agent-child"
            && input.content == piko_protocol::MessageContent::String("next task".into())
            && input.delivery == piko_protocol::AgentInputDelivery::FollowUp
    ));
}

#[test]
fn running_guidance_names_steer_and_queue_keys() {
    use crate::features::guidance_row::{GuidanceContent, resolve};

    let app = running_app();
    let GuidanceContent::Hint(hint) = resolve(&app) else {
        panic!("expected composer hint");
    };
    assert!(hint.contains("Enter steer"), "{hint}");
    assert!(hint.contains("Alt+Enter queue"), "{hint}");
}

#[test]
fn queued_guidance_and_summary_show_follow_up_count() {
    use crate::features::guidance_row::{GuidanceContent, resolve};

    let mut app = running_app();
    app.session.agent_work.insert(
        "task-1".into(),
        work_snapshot(
            Some("input-live"),
            vec![queued_input("input-later", "later")],
        ),
    );
    app.queue_status.follow_up_count = 1;
    assert_eq!(app.queue_summary().follow_up_count, 1);
    let GuidanceContent::Hint(hint) = resolve(&app) else {
        panic!("expected composer hint");
    };
    assert!(hint.contains("1 queued"), "{hint}");
}

#[test]
fn enter_while_running_sends_queue_steer() {
    let mut app = running_app();
    app.editor.restore_text("change course");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInputSubmit {
            input,
            ..
        })] if input.agent_instance_id == "task-1"
            && input.content == piko_protocol::MessageContent::String("change course".into())
            && input.delivery == piko_protocol::AgentInputDelivery::SteerActive
    ));
    assert!(app.editor.is_empty());
}

#[test]
fn image_while_running_sends_structured_steer() {
    let mut app = running_app();
    app.editor
        .insert_image("clipboard.png", "AA==".into(), "image/png".into());

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInputSubmit {
            input: piko_protocol::AgentInput {
                content: piko_protocol::MessageContent::Blocks(blocks),
                delivery: piko_protocol::AgentInputDelivery::SteerActive,
                ..
            },
            ..
        })] if matches!(blocks.as_slice(), [piko_protocol::ContentBlock::Image { data, .. }] if data == "AA==")
    ));
}

#[test]
fn alt_enter_while_running_queues_follow_up() {
    let mut app = running_app();
    app.editor.restore_text("do this next");

    let effects = app.dispatch(EditorAction::FollowUp.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInputSubmit {
            input,
            ..
        })] if input.agent_instance_id == "task-1"
            && input.content == piko_protocol::MessageContent::String("do this next".into())
            && input.delivery == piko_protocol::AgentInputDelivery::FollowUp
    ));
    assert_eq!(app.queue_summary().follow_up_count, 0);
}

#[test]
fn rejected_follow_up_restores_draft_and_rolls_back_local_queue() {
    let mut app = running_app();
    app.editor.restore_text("do this next");
    let effects = app.dispatch(EditorAction::FollowUp.into());
    let command_id = match effects.as_slice() {
        [Effect::Send(piko_protocol::Command::AgentInputSubmit { command_id, .. })] => {
            command_id.clone()
        }
        other => panic!("unexpected effects: {other:?}"),
    };

    app.handle_host_line(crate::host::HostLine::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id,
            result: Err("rejected".into()),
        },
    )));

    assert_eq!(app.editor.text(), "do this next");
}

#[test]
fn steer_while_idle_keeps_draft() {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("task-1".into());
    app.editor.restore_text("too early");

    let effects = app.dispatch(EditorAction::Steer.into());

    assert!(effects.is_empty());
    assert_eq!(app.editor.text(), "too early");
    assert!(app.status.contains("not running"));
}

#[test]
fn queued_event_does_not_replace_running_work() {
    let mut app = running_app();

    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
        session_id: "session-1".into(),
        turn_id: "turn-queued".into(),
        agent_instance_id: "task-1".into(),
        timestamp: 0,
    }));

    assert_eq!(
        app.session.agent_work["task-1"]
            .active_work
            .as_ref()
            .map(|work| work.root_input_id.as_str()),
        Some("input-live")
    );
}

#[test]
fn dequeue_restores_preview_and_cancels_authoritative_input() {
    let mut app = running_app();
    app.session.agent_work.insert(
        "task-1".into(),
        work_snapshot(
            Some("input-live"),
            vec![queued_input("input-queued", "bring back")],
        ),
    );

    let effects = app.dispatch(EditorAction::DequeueFollowUp.into());

    assert_eq!(app.editor.text(), "bring back");
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::AgentInputCancel { input_id, .. })]
            if input_id == "input-queued"
    ));
}
