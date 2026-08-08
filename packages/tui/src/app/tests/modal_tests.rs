use std::path::PathBuf;

use crossterm::event::{KeyEventKind, KeyEventState};

use crate::app::{AppMode, AppState, InitialOptions, SurfaceId, command::ToolInteractionAction};
use crate::features::approval::PendingApproval;
use crate::features::tool_interaction::ToolInteractionPanel;
use crate::input::focus::InputRouter;
use crate::input::keymap::Keymap;
use crate::navigation::FocusManagerExt;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

fn key(
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn push_approval(app: &mut AppState) {
    app.approvals.push(PendingApproval {
        id: "a1".into(),
        agent_instance_id: "agent-1".into(),
        tool_name: "bash".into(),
        args: serde_json::json!({ "command": "cargo test" }),
        prompt: None,
    });
    app.push_surface(SurfaceId::Approval);
}

#[test]
fn pending_approval_rejects_other_surface_pushes() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    push_approval(&mut app);
    assert_eq!(app.modal_surface(), Some(SurfaceId::Approval));

    // A Decide surface is a focus barrier: opening the tree is rejected.
    app.push_surface(SurfaceId::Tree);
    assert_eq!(app.mode, AppMode::Surface(SurfaceId::Approval));

    // A queued Tool Interaction does not steal focus either.
    app.interactions = ToolInteractionPanel::new();
    app.interactions
        .push("i1".into(), "agent-1".into(), None, Vec::new(), false, true);
    app.push_surface(SurfaceId::ToolInteraction);
    assert_eq!(app.mode, AppMode::Surface(SurfaceId::Approval));
    assert_eq!(app.modal_surface(), Some(SurfaceId::Approval));
}

#[test]
fn f4_does_not_steal_focus_from_pending_decide() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    push_approval(&mut app);

    let keymap = Keymap::default();
    let action = InputRouter::route_key(
        &mut app,
        &keymap,
        key(
            crossterm::event::KeyCode::F(4),
            crossterm::event::KeyModifiers::NONE,
        ),
    );

    // F4 is consumed but must NOT open the Agents surface.
    assert!(action.is_none());
    assert_eq!(app.modal_surface(), Some(SurfaceId::Approval));
    assert_eq!(
        app.focus_manager.active_surface(),
        Some(SurfaceId::Approval)
    );
}

#[test]
fn approval_letter_shortcuts_ignore_modified_chords() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    push_approval(&mut app);

    let keymap = Keymap::default();
    // ctrl+a must NOT resolve AcceptSession.
    let action = InputRouter::route_key(
        &mut app,
        &keymap,
        key(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    );
    assert!(action.is_none());

    // Plain 'a' still resolves AcceptSession.
    let action = InputRouter::route_key(
        &mut app,
        &keymap,
        key(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ),
    );
    assert!(matches!(
        action,
        Some(crate::app::command::Action::Approval(
            crate::app::command::ApprovalAction::Respond(
                piko_protocol::ApprovalDecision::AcceptSession
            )
        ))
    ));
}

#[test]
fn tool_interaction_arrows_move_choices_tab_moves_steps() {
    use crate::ui::components::interactive_workflow::{
        ChoiceOption, InteractiveWorkflow, Question,
    };

    let mut app = app();
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
        .push("i1".into(), "agent-1".into(), None, Vec::new(), false, true);
    app.interactions
        .front_mut()
        .expect("front interaction")
        .workflow = workflow;
    app.push_surface(SurfaceId::ToolInteraction);

    let keymap = Keymap::default();
    // Down moves to the next choice within the question.
    let action = InputRouter::route_key(
        &mut app,
        &keymap,
        key(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ),
    );
    let action = action.expect("down routes to SelectNext");
    assert!(matches!(
        action,
        crate::app::command::Action::ToolInteraction(ToolInteractionAction::SelectNext)
    ));
    app.dispatch(action);
    let front = app.interactions.front().expect("front");
    assert_eq!(front.workflow.questions[0].selected_idx, 1);

    // Tab moves to the next question.
    let action = InputRouter::route_key(
        &mut app,
        &keymap,
        key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ),
    );
    let action = action.expect("tab routes to NextStep");
    assert!(matches!(
        action,
        crate::app::command::Action::ToolInteraction(ToolInteractionAction::NextStep)
    ));
    app.dispatch(action);
    let front = app.interactions.front().expect("front");
    assert_eq!(front.workflow.active_question_idx, 1);
}

#[test]
fn workflow_locks_input_after_submit_and_cancel() {
    use crate::ui::components::interactive_workflow::{
        ChoiceOption, InteractiveWorkflow, Question,
    };

    let mut app = app();
    app.session.id = Some("session-1".into());
    app.interactions = ToolInteractionPanel::new();
    app.interactions
        .push("i1".into(), "agent-1".into(), None, Vec::new(), false, true);
    app.interactions.front_mut().expect("front").workflow = InteractiveWorkflow::new(
        vec![Question::new(
            "Go",
            "continue?",
            vec![ChoiceOption {
                label: "yes".into(),
                has_input: false,
                input_prompt: String::new(),
            }],
        )],
        false,
    );
    app.push_surface(SurfaceId::ToolInteraction);

    let first = app.dispatch(crate::app::command::Action::ToolInteraction(
        ToolInteractionAction::Submit,
    ));
    assert!(!first.is_empty(), "first submit sends the response");
    assert!(app.interactions.front().is_some_and(|i| i.submitting));

    let second = app.dispatch(crate::app::command::Action::ToolInteraction(
        ToolInteractionAction::Submit,
    ));
    assert!(second.is_empty(), "no double submit while pending");

    let cancel = app.dispatch(crate::app::command::Action::ToolInteraction(
        ToolInteractionAction::Cancel,
    ));
    assert!(cancel.is_empty(), "no cancel after submit");
}
