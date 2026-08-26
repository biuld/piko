use super::*;
use crate::app::command::EditorAction;
use piko_protocol::TurnStatus;

fn running_app() -> AppState {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("task-1".into());
    app.session.active_turns.insert(
        "task-1".into(),
        crate::app::ActiveTurnUi {
            turn_id: "turn-live".into(),
            status: TurnStatus::Running,
        },
    );
    app
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
    app.session.follow_ups.push(crate::app::FollowUpUi {
        command_id: None,
        agent_instance_id: "task-1".into(),
        text: "later".into(),
        content: piko_protocol::MessageContent::String("later".into()),
        turn_id: None,
        cancel_when_queued: false,
    });
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
        [Effect::Send(piko_protocol::Command::QueueSteer {
            agent_instance_id,
            message,
            ..
        })] if agent_instance_id == "task-1" && message == "change course"
    ));
    assert!(app.editor.is_empty());
    assert!(app.session.follow_ups.is_empty());
}

#[test]
fn image_while_running_sends_structured_steer() {
    let mut app = running_app();
    app.editor
        .insert_image("clipboard.png", "AA==".into(), "image/png".into());

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::QueueSteerMessage {
            content: piko_protocol::MessageContent::Blocks(blocks),
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
        [Effect::Send(piko_protocol::Command::ChatSubmit {
            target_agent_instance_id,
            text,
            ..
        })] if target_agent_instance_id == "task-1" && text == "do this next"
    ));
    assert_eq!(app.session.follow_ups.len(), 1);
    assert_eq!(app.session.follow_ups[0].text, "do this next");
    assert_eq!(app.queue_summary().follow_up_count, 1);
}

#[test]
fn rejected_follow_up_restores_draft_and_rolls_back_local_queue() {
    let mut app = running_app();
    app.editor.restore_text("do this next");
    let effects = app.dispatch(EditorAction::FollowUp.into());
    let command_id = match effects.as_slice() {
        [Effect::Send(piko_protocol::Command::ChatSubmit { command_id, .. })] => command_id.clone(),
        other => panic!("unexpected effects: {other:?}"),
    };

    app.handle_host_line(crate::host::HostLine::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id,
            result: Err("rejected".into()),
        },
    )));

    assert_eq!(app.editor.text(), "do this next");
    assert!(app.session.follow_ups.is_empty());
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
fn queued_event_does_not_replace_running_turn() {
    let mut app = running_app();
    app.session.follow_ups.push(crate::app::FollowUpUi {
        command_id: None,
        agent_instance_id: "task-1".into(),
        text: "later".into(),
        content: piko_protocol::MessageContent::String("later".into()),
        turn_id: None,
        cancel_when_queued: false,
    });

    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
        session_id: "session-1".into(),
        turn_id: "turn-queued".into(),
        agent_instance_id: "task-1".into(),
        timestamp: 0,
    }));

    assert_eq!(app.session.active_turns["task-1"].turn_id, "turn-live");
    assert_eq!(
        app.session.active_turns["task-1"].status,
        TurnStatus::Running
    );
    assert_eq!(
        app.session.follow_ups[0].turn_id.as_deref(),
        Some("turn-queued")
    );
}

#[test]
fn dequeue_restores_text_and_cancels_queued_turn() {
    let mut app = running_app();
    app.session.follow_ups.push(crate::app::FollowUpUi {
        command_id: None,
        agent_instance_id: "task-1".into(),
        text: "bring back".into(),
        content: piko_protocol::MessageContent::String("bring back".into()),
        turn_id: Some("turn-queued".into()),
        cancel_when_queued: false,
    });

    let effects = app.dispatch(EditorAction::DequeueFollowUp.into());

    assert_eq!(app.editor.text(), "bring back");
    assert!(app.session.follow_ups.is_empty());
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::TurnCancel { turn_id, .. })]
            if turn_id == "turn-queued"
    ));
}

#[test]
fn dequeue_waits_for_queued_event_then_cancels() {
    let mut app = running_app();
    app.session.follow_ups.push(crate::app::FollowUpUi {
        command_id: None,
        agent_instance_id: "task-1".into(),
        text: "pending id".into(),
        content: piko_protocol::MessageContent::String("pending id".into()),
        turn_id: None,
        cancel_when_queued: false,
    });

    assert!(
        app.dispatch(EditorAction::DequeueFollowUp.into())
            .is_empty()
    );
    assert_eq!(app.editor.text(), "pending id");
    assert!(app.session.follow_ups[0].cancel_when_queued);

    let effects = app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
        session_id: "session-1".into(),
        turn_id: "turn-late".into(),
        agent_instance_id: "task-1".into(),
        timestamp: 0,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::TurnCancel { turn_id, .. })]
            if turn_id == "turn-late"
    ));
    assert!(app.session.follow_ups.is_empty());
}
