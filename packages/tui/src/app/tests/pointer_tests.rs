#![allow(unused_imports)]

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use piko_protocol::{ApprovalDecision, TodoItem, TodoList, TodoStatus};
use ratatui::layout::Rect;

use crate::app::{
    AppState, HitId, InitialOptions, SurfaceId, ToolStatus,
    command::{
        Action, ApprovalAction, EditorAction, NotificationAction, SurfaceAction, TimelineAction,
        ToolInteractionAction,
    },
    effect::Effect,
};
use crate::features::approval::PendingApproval;
use crate::features::notifications::NotificationLevel;
use crate::features::timeline::{ThoughtKey, TimelineEntry, ToolEntry};
use crate::features::tool_interaction::ToolInteractionPanel;
use crate::input::pointer::route_pointer;
use crate::layout::build_surface_hitmap;
use crate::layout::compose_frame;
use crate::navigation::Region;
use crate::ui::components::choice_workflow::{ChoiceOption, ChoiceWorkflow, Question};
use crate::ui::components::pane::PaneSpec;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn push_approval(app: &mut AppState) {
    app.session.id = Some("session-1".into());
    app.approvals.push(PendingApproval {
        id: "a1".into(),
        agent_instance_id: "agent-1".into(),
        tool_name: "bash".into(),
        args: serde_json::json!({ "command": "cargo test" }),
        prompt: None,
        selected_idx: 0,
    });
    app.push_surface(SurfaceId::Approval);
}

fn approval_host(app: &AppState) -> Rect {
    compose_frame(app, Rect::new(0, 0, 80, 24))
        .plan
        .layers
        .first()
        .and_then(|l| l.rects.get(&Region::Surface(SurfaceId::Approval)))
        .copied()
        .expect("approval host")
}

#[test]
fn approval_choice_click_resolves_the_matching_decision() {
    let mut app = app();
    push_approval(&mut app);
    let host = approval_host(&app);

    // Standard pane chrome: content.y = host.y + 2; single-question layout →
    // choices at content.y + 2 = host.y + 4.
    let decisions = [
        ApprovalDecision::Accept,
        ApprovalDecision::AcceptSession,
        ApprovalDecision::AcceptWorkspace,
        ApprovalDecision::AcceptPermanent,
        ApprovalDecision::Decline,
    ];
    for (i, expected) in decisions.iter().enumerate() {
        let actions = route_pointer(
            &mut app,
            Rect::new(0, 0, 80, 24),
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                host.x + 2,
                host.y + 4 + i as u16,
            ),
        );
        assert_eq!(actions.len(), 1, "choice {i} resolves exactly one action");
        assert!(matches!(
            &actions[0],
            Action::Approval(ApprovalAction::Respond(decision)) if decision == expected
        ));
    }
}

#[test]
fn approval_background_click_is_ignored() {
    let mut app = app();
    push_approval(&mut app);
    let host = approval_host(&app);
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            host.x + 2,
            host.y + host.height - 1,
        ),
    );
    assert!(actions.is_empty());
}

#[test]
fn blocking_dock_outside_click_does_not_fall_through_or_close() {
    let mut app = app();
    push_approval(&mut app);

    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
    );

    assert!(actions.is_empty());
    assert_eq!(app.modal_surface(), Some(SurfaceId::Approval));
}

#[test]
fn dismissible_modal_outside_click_maps_to_keyboard_close() {
    let mut app = app();
    app.push_surface(SurfaceId::Settings);
    let host = compose_frame(&app, Rect::new(0, 0, 80, 24))
        .plan
        .layers
        .first()
        .and_then(|layer| layer.rects.get(&Region::Surface(SurfaceId::Settings)))
        .copied()
        .expect("settings host");
    let outside_x = host.x.saturating_sub(1);

    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), outside_x, host.y),
    );

    assert!(matches!(
        actions.as_slice(),
        [Action::Surface(SurfaceAction::Close)]
    ));
}

#[test]
fn hover_outside_modal_does_not_expose_lower_layer_target() {
    let mut app = app();
    app.push_surface(SurfaceId::Settings);

    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Moved, 1, 1),
    );

    assert!(actions.is_empty());
    assert_eq!(app.hovered, None);
}

#[test]
fn dismissing_thought_overlay_does_not_reopen_it_on_release() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.apply_event(super::realtime(
        "message-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(super::realtime(
        "message-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Thinking {
            content_index: 0,
            delta: "thought".into(),
        },
    ));

    let key = ThoughtKey {
        message_id: "message-1".into(),
        segment_index: 0,
    };
    let hit_id = app.timeline().thought_hit_id(&key).expect("thought hit id");
    let terminal = Rect::new(0, 0, 80, 24);
    let frame = crate::layout::prepare_frame(&app, terminal);
    let plan = frame.timeline.as_ref().expect("timeline plan");
    let (x, y) = (plan.content_area.x, plan.content_area.y);
    assert!(matches!(
        plan.resolve(x, y, app.timeline().viewport.top_offset()),
        Some((HitId::TimelineThought(id), _)) if id == hit_id
    ));

    let _ = app.dispatch(TimelineAction::OpenThought(hit_id).into());
    assert_eq!(app.modal_surface(), Some(SurfaceId::ThoughtInspector));

    let down = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(matches!(
        down.as_slice(),
        [Action::Surface(SurfaceAction::Close)]
    ));
    for action in down {
        let _ = app.dispatch(action);
    }
    assert_eq!(app.modal_surface(), None);

    let up = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    assert!(up.is_empty());
    assert_eq!(app.modal_surface(), None);
}

fn push_workflow(app: &mut AppState) {
    app.session.id = Some("session-1".into());
    let workflow = ChoiceWorkflow::new(
        vec![
            Question::new(
                "Scope",
                "choose scope",
                vec![
                    ChoiceOption {
                        label: "one".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                    ChoiceOption {
                        label: "two".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    },
                ],
            ),
            Question::new(
                "Format",
                "choose format",
                vec![ChoiceOption {
                    label: "concise".into(),
                    has_input: false,
                    input_prompt: String::new(),
                }],
            ),
        ],
        true,
    );
    app.interactions = ToolInteractionPanel::new();
    app.interactions
        .push("i1".into(), "agent-1".into(), None, Vec::new(), true, true);
    app.interactions.front_mut().expect("front").workflow = workflow;
    app.push_surface(SurfaceId::ToolInteraction);
}

fn workflow_host(app: &AppState) -> Rect {
    compose_frame(app, Rect::new(0, 0, 80, 24))
        .plan
        .layers
        .first()
        .and_then(|l| l.rects.get(&Region::Surface(SurfaceId::ToolInteraction)))
        .copied()
        .expect("workflow host")
}

#[test]
fn workflow_choice_click_selects_then_submits_like_enter() {
    let mut app = app();
    push_workflow(&mut app);
    let host = workflow_host(&app);
    // Standard pane chrome: content.y = host.y + 2; choices of the active
    // question start at content.y + 4 = host.y + 6 (tabs, blank, prompt,
    // blank).
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            host.x + 2,
            host.y + 6,
        ),
    );
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        &actions[0],
        Action::ToolInteraction(ToolInteractionAction::Choice(0))
    ));
    assert!(matches!(
        &actions[1],
        Action::ToolInteraction(ToolInteractionAction::Submit)
    ));
}

#[test]
fn workflow_tab_click_jumps_to_that_step() {
    let mut app = app();
    push_workflow(&mut app);
    let host = workflow_host(&app);
    // Tab row at content.y = host.y + 2: [Scope] (width 7) at content.x
    // (host.x + 2), then 3 spaces, then [Format] (width 8) at +10.
    let inner_x = host.x + 2;
    let format_x = inner_x + 10;
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            format_x,
            host.y + 2,
        ),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::ToolInteraction(ToolInteractionAction::GotoStep(1))]
    ));
}

#[test]
fn wheel_over_stream_scrolls_timeline() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::ScrollDown, 5, 5),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::ScrollDown(3))]
    ));
}

#[test]
fn wheel_outside_stream_is_ignored() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::ScrollUp, 40, 23), // chrome row
    );
    assert!(actions.is_empty());
}

#[test]
fn timeline_tool_block_hit_wins_over_stream_and_toggles_that_block() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "tool-1".into(),
        "bash".into(),
        ToolStatus::Completed,
        r#"{"cmd":"true"}"#.into(),
        Some("done".into()),
        None,
    )));
    let terminal = Rect::new(0, 0, 80, 24);
    let stream = compose_frame(&app, terminal).plan.rects[&Region::Stream];
    let (hit_id, tool) = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .first()
        .copied()
        .expect("timeline tool hit");
    let x = tool.x;
    let y = tool.y;

    let hover = route_pointer(&mut app, terminal, mouse(MouseEventKind::Moved, x, y));
    assert!(hover.is_empty());
    assert_eq!(
        app.hovered,
        Some((
            Region::Stream,
            Some(crate::app::HitId::TimelineTool(hit_id))
        ))
    );

    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::SelectionStart(_))]
    ));
    for action in actions {
        let _ = app.dispatch(action);
    }
    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::SelectionFinish {
            activation: Some(crate::app::command::TimelineActivation::Tool(id)), ..
        })] if *id == hit_id
    ));
}

#[test]
fn composer_click_places_cursor_without_modal() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.editor.restore_text("hello world");
    let composer = compose_frame(&app, Rect::new(0, 0, 80, 24))
        .plan
        .rects
        .get(&Region::Composer)
        .copied()
        .expect("composer rect");
    route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            composer.x + 2,
            composer.y + 1,
        ),
    );
    assert_eq!(app.editor.cursor(), 2);
}

#[test]
fn notice_click_clears_notifications() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.notifications
        .push(NotificationLevel::Warning, "something needs attention");
    let notice = compose_frame(&app, Rect::new(0, 0, 80, 24))
        .plan
        .rects
        .get(&Region::Guidance)
        .copied()
        .expect("notice rect");
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            notice.x + 1,
            notice.y,
        ),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Notifications(NotificationAction::DismissVisible)]
    ));
}
