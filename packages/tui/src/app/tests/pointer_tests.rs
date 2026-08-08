use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use piko_protocol::ApprovalDecision;
use ratatui::layout::Rect;

use crate::app::{
    AppState, InitialOptions, SurfaceId,
    command::{
        Action, ApprovalAction, EditorAction, NotificationAction, TimelineAction,
        ToolInteractionAction,
    },
};
use crate::features::approval::PendingApproval;
use crate::features::notifications::NotificationLevel;
use crate::features::tool_interaction::ToolInteractionPanel;
use crate::input::pointer::route_pointer;
use crate::layout::compose_frame;
use crate::navigation::Region;
use crate::ui::components::interactive_workflow::{ChoiceOption, InteractiveWorkflow, Question};
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

fn push_workflow(app: &mut AppState) {
    app.session.id = Some("session-1".into());
    let workflow = InteractiveWorkflow::new(
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
        .get(&Region::Notice)
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
        [Action::Notifications(NotificationAction::Clear)]
    ));
}

#[test]
fn hover_updates_hovered_state_without_actions() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Moved, 5, 5),
    );
    assert!(actions.is_empty());
    assert_eq!(app.hovered.map(|(region, _)| region), Some(Region::Stream));
}

#[test]
fn suggestion_click_accepts_that_row() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    super::with_local_slash_catalog(&mut app);
    app.editor.restore_text("/r");
    app.refresh_suggestions();
    assert!(app.editor.auto_complete.is_active());

    let suggest = compose_frame(&app, Rect::new(0, 0, 80, 24))
        .plan
        .rects
        .get(&Region::Suggest)
        .copied()
        .expect("suggest rect");
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            suggest.x + 1,
            suggest.y + 1,
        ),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Editor(EditorAction::AcceptSuggestion)]
    ));
    assert_eq!(app.editor.auto_complete.list.selected, 0);
}

#[test]
fn workflow_inline_input_cursor_tracks_caret() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.interactions = ToolInteractionPanel::new();
    app.interactions
        .push("i1".into(), "agent-1".into(), None, Vec::new(), false, true);
    let workflow = InteractiveWorkflow::new(
        vec![Question::new(
            "Go",
            "continue?",
            vec![ChoiceOption {
                label: "custom".into(),
                has_input: true,
                input_prompt: "value".into(),
            }],
        )],
        false,
    );
    app.interactions.front_mut().expect("front").workflow = workflow;
    app.push_surface(SurfaceId::ToolInteraction);
    {
        let interaction = app.interactions.front_mut().unwrap();
        interaction.workflow.set_input_active(true);
        interaction.workflow.questions[0]
            .input_value
            .insert_char('a');
        interaction.workflow.questions[0]
            .input_value
            .insert_char('b');
    }

    let host = workflow_host(&app);
    let position = app
        .interactions
        .front()
        .unwrap()
        .workflow
        .input_cursor(host)
        .expect("inline input cursor");
    let help = app.interactions.front().unwrap().workflow.help_text();
    let content = PaneSpec::new("")
        .hints(&help)
        .content_rect(host)
        .expect("content rect");
    // Single-question layout: choice row is content.y + 2 (prompt, blank).
    assert_eq!(position.y, content.y + 2);
    // x = content.x + prefix(2) + "1. "(3) + "custom"(6) + ": "(2) + "ab"(2).
    assert_eq!(position.x, content.x + 2 + 3 + 6 + 2 + 2);
}
