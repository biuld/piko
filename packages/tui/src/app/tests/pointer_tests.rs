use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use piko_protocol::ApprovalDecision;
use ratatui::layout::Rect;

use crate::app::{
    AppState, InitialOptions, SurfaceId, ToolStatus,
    command::{
        Action, ApprovalAction, EditorAction, NotificationAction, SurfaceAction, TimelineAction,
        ToolInteractionAction,
    },
};
use crate::features::approval::PendingApproval;
use crate::features::notifications::NotificationLevel;
use crate::features::timeline::{TimelineEntry, ToolEntry};
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

fn element_point(app: &AppState, surface: SurfaceId, element: crate::app::HitId) -> (u16, u16) {
    let map = build_surface_hitmap(app, Rect::new(0, 0, 80, 24));
    let hit = map
        .hits
        .iter()
        .find(|hit| hit.region == Region::Surface(surface) && hit.element == Some(element))
        .expect("surface element hit");
    (hit.rect.x, hit.rect.y)
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
    app.timeline.push(TimelineEntry::Tool(ToolEntry::new(
        "tool-1".into(),
        "bash".into(),
        ToolStatus::Completed,
        r#"{"cmd":"true"}"#.into(),
        Some("done".into()),
        None,
    )));
    let terminal = Rect::new(0, 0, 80, 24);
    let map = build_surface_hitmap(&app, terminal);
    let tool = map
        .hits
        .iter()
        .find(|hit| hit.element == Some(crate::app::HitId::TimelineTool(0)))
        .expect("timeline tool hit");
    let x = tool.rect.x;
    let y = tool.rect.y;

    let hover = route_pointer(&mut app, terminal, mouse(MouseEventKind::Moved, x, y));
    assert!(hover.is_empty());
    assert_eq!(
        app.hovered,
        Some((Region::Stream, Some(crate::app::HitId::TimelineTool(0))))
    );

    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::ToggleTool(0))]
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
        [Action::Notifications(NotificationAction::DismissVisible)]
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
        [Action::Editor(EditorAction::AcceptAndSubmitSuggestion)]
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
    let workflow = ChoiceWorkflow::new(
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

    let (x, y) = element_point(
        &app,
        SurfaceId::ToolInteraction,
        crate::app::HitId::TextInput,
    );
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(actions.is_empty());
    assert_eq!(
        app.interactions.front().unwrap().workflow.questions[0]
            .input_value
            .width_before_cursor(),
        0
    );
}

#[test]
fn model_row_click_selects_and_uses_confirm_action() {
    use crate::features::model_selector::ModelOption;
    let mut app = app();
    app.models.load(vec![
        ModelOption {
            provider: "one".into(),
            id: "a".into(),
            name: "A".into(),
            has_auth: true,
        },
        ModelOption {
            provider: "two".into(),
            id: "b".into(),
            name: "B".into(),
            has_auth: true,
        },
    ]);
    app.push_surface(SurfaceId::Models);
    let (x, y) = element_point(&app, SurfaceId::Models, crate::app::HitId::Row(1));
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert_eq!(app.models.list.selected, 1);
    assert!(matches!(
        actions.as_slice(),
        [Action::Surface(SurfaceAction::Confirm)]
    ));
}

#[test]
fn mcp_row_click_only_moves_read_only_selection() {
    use piko_protocol::command::McpServerInfo;
    let mut app = app();
    let server = |name: &str| McpServerInfo {
        name: name.into(),
        connected: true,
        tool_count: 0,
        resource_count: 0,
        template_count: 0,
        error: None,
    };
    app.mcp.set_servers(vec![server("one"), server("two")]);
    app.push_surface(SurfaceId::Mcp);
    let (x, y) = element_point(&app, SurfaceId::Mcp, crate::app::HitId::Row(1));
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(actions.is_empty());
    assert_eq!(app.mcp.selected_index(), 1);
}

#[test]
fn summary_prompt_choice_click_uses_embedded_workflow() {
    let mut app = app();
    app.summary_prompt = Some(ChoiceWorkflow::new(
        vec![Question::new(
            "Summary",
            "choose",
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
        )],
        false,
    ));
    app.push_surface(SurfaceId::SummaryPrompt);
    let target = crate::app::HitId::Choice {
        question: 0,
        choice: 1,
    };
    let (x, y) = element_point(&app, SurfaceId::SummaryPrompt, target);
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert_eq!(
        app.summary_prompt.as_ref().unwrap().questions[0].selected_idx,
        1
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Surface(SurfaceAction::Confirm)]
    ));
}
