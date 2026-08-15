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

fn workflow_host(app: &AppState) -> Rect {
    compose_frame(app, Rect::new(0, 0, 80, 24))
        .plan
        .layers
        .first()
        .and_then(|layer| {
            layer
                .rects
                .get(&Region::Surface(SurfaceId::ToolInteraction))
        })
        .copied()
        .expect("workflow host")
}

#[test]
fn todos_header_hitzone_toggles_one_line_summary() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(TodoList {
        agent_instance_id: "agent-1".into(),
        items: vec![
            TodoItem {
                id: "1".into(),
                status: TodoStatus::InProgress,
                content: "active item".into(),
                detail: None,
            },
            TodoItem {
                id: "2".into(),
                status: TodoStatus::Pending,
                content: "pending item".into(),
                detail: None,
            },
        ],
        updated_at: 1,
        revision: 1,
    });

    let terminal = Rect::new(0, 0, 80, 24);
    let expanded = compose_frame(&app, terminal).plan.rects[&Region::Todos];
    assert_eq!(expanded.height, 4); // header + two items + separator

    let header = build_surface_hitmap(&app, terminal)
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::TodosToggle))
        .expect("todos header hitzone");
    assert_eq!(header.rect.height, 1);
    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            header.rect.x,
            header.rect.y,
        ),
    );
    assert!(actions.is_empty());
    assert!(app.todo_lists.is_collapsed());
    assert_eq!(
        compose_frame(&app, terminal).plan.rects[&Region::Todos].height,
        2
    );

    let collapsed_header = build_surface_hitmap(&app, terminal)
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::TodosToggle))
        .expect("collapsed todos header hitzone");
    route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            collapsed_header.rect.x,
            collapsed_header.rect.y,
        ),
    );
    assert!(!app.todo_lists.is_collapsed());

    // Item rows remain read-only and do not share the header toggle hitzone.
    route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            expanded.x,
            expanded.y + 1,
        ),
    );
    assert!(!app.todo_lists.is_collapsed());
}

#[test]
fn release_only_terminal_events_activate_timeline_and_todos() {
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
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(TodoList {
        agent_instance_id: "agent-1".into(),
        items: vec![TodoItem {
            id: "1".into(),
            status: TodoStatus::Pending,
            content: "pending item".into(),
            detail: None,
        }],
        updated_at: 1,
        revision: 1,
    });

    let terminal = Rect::new(0, 0, 80, 24);
    let map = build_surface_hitmap(&app, terminal);
    let tool = map
        .hits
        .iter()
        .find(|hit| hit.element == Some(HitId::TimelineTool(0)))
        .expect("timeline tool hitzone");
    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            tool.rect.x,
            tool.rect.y,
        ),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::ToggleTool(0))]
    ));

    let todo = build_surface_hitmap(&app, terminal)
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::TodosToggle))
        .expect("todo hitzone");
    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            todo.rect.x,
            todo.rect.y,
        ),
    );
    assert!(actions.is_empty());
    assert!(app.todo_lists.is_collapsed());
}

#[test]
fn paired_release_does_not_toggle_todos_twice() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(TodoList {
        agent_instance_id: "agent-1".into(),
        items: vec![TodoItem {
            id: "1".into(),
            status: TodoStatus::Pending,
            content: "pending item".into(),
            detail: None,
        }],
        updated_at: 1,
        revision: 1,
    });

    let terminal = Rect::new(0, 0, 80, 24);
    let todo = build_surface_hitmap(&app, terminal)
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::TodosToggle))
        .expect("todo hitzone");
    let (x, y) = (todo.rect.x, todo.rect.y);
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    assert!(app.todo_lists.is_collapsed());
}

#[test]
fn notification_copy_button_targets_the_complete_original_message() {
    let mut app = app();
    let id = app
        .notifications
        .push(NotificationLevel::Error, "first line\nsecond line");
    app.notifications.open_modal();
    app.push_surface(SurfaceId::Notifications);
    let (x, y) = element_point(
        &app,
        SurfaceId::Notifications,
        crate::app::HitId::NotificationCopy(id),
    );

    let mut actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    let effects = app.dispatch(actions.pop().expect("copy action"));

    assert!(matches!(
        effects.as_slice(),
        [Effect::CopyToClipboard { notification_id, text }]
            if *notification_id == id && text == "first line\nsecond line"
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

    let row = build_surface_hitmap(&app, Rect::new(0, 0, 80, 24))
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::Suggest(0)))
        .expect("first suggestion hit row")
        .rect;
    let actions = route_pointer(
        &mut app,
        Rect::new(0, 0, 80, 24),
        mouse(MouseEventKind::Down(MouseButton::Left), row.x, row.y),
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
        .hints(help.as_str())
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
            reasoning_efforts: vec![],
        },
        ModelOption {
            provider: "two".into(),
            id: "b".into(),
            name: "B".into(),
            has_auth: true,
            reasoning_efforts: vec![],
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
