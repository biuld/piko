use std::path::PathBuf;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::*;
use crate::{
    app::InitialOptions,
    features::{approval::PendingApproval, notifications::NotificationLevel},
    layout::{build_surface_hitmap, compose_frame},
};

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-render-interaction-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

fn draw(app: &AppState, area: Rect) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal
}

#[test]
fn notice_component_paints_its_hover_background() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.notifications
        .push(NotificationLevel::Warning, "click to dismiss");
    app.hovered = Some((Region::Notice, Some(HitId::Notice)));
    let area = Rect::new(0, 0, 80, 24);
    let notice = compose_frame(&app, area).plan.rects[&Region::Notice];

    let terminal = draw(&app, area);

    assert_eq!(
        terminal.backend().buffer()[(notice.x + 1, notice.y)].bg,
        app.theme.bg_hover
    );
}

fn assert_editor_focus_feedback(mut app: AppState) {
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];

    let idle = draw(&app, area);
    assert_eq!(
        idle.backend().buffer()[(composer.x + 1, composer.y + 1)].bg,
        app.theme.bg_elevated
    );
    assert_eq!(
        idle.backend().buffer()[(composer.x, composer.y)].fg,
        app.theme.prompt_border_active
    );

    app.hovered = Some((Region::Composer, Some(HitId::Composer)));
    let hovered = draw(&app, area);

    assert_eq!(
        hovered.backend().buffer()[(composer.x + 1, composer.y + 1)].bg,
        app.theme.bg_elevated
    );
    assert_eq!(
        hovered.backend().buffer()[(composer.x, composer.y)].fg,
        app.theme.prompt_border_active
    );
}

#[test]
fn editor_component_ignores_hover_and_keeps_focus_feedback_in_dark_theme() {
    assert_editor_focus_feedback(app());
}

#[test]
fn editor_component_ignores_hover_and_keeps_focus_feedback_in_light_theme() {
    let mut light = app();
    light.theme = crate::theme::Theme::light();
    assert_editor_focus_feedback(light);
}

#[test]
fn workflow_component_keeps_selection_above_hover() {
    let mut app = app();
    app.approvals.push(PendingApproval {
        id: "approval-1".into(),
        agent_instance_id: "agent-1".into(),
        tool_name: "bash".into(),
        args: serde_json::json!({ "command": "cargo test" }),
        prompt: None,
    });
    app.push_surface(SurfaceId::Approval);
    let area = Rect::new(0, 0, 80, 24);
    let map = build_surface_hitmap(&app, area);
    let hovered = HitId::Choice {
        question: 0,
        choice: 1,
    };
    app.hovered = Some((Region::Surface(SurfaceId::Approval), Some(hovered)));
    let hovered_rect = map
        .hits
        .iter()
        .find(|hit| hit.element == Some(hovered))
        .unwrap()
        .rect;

    let terminal = draw(&app, area);

    assert_eq!(
        terminal.backend().buffer()[(hovered_rect.x, hovered_rect.y)].bg,
        app.theme.bg_hover
    );
    let workflow = app.approvals.workflow().unwrap();
    let selected_rect = workflow
        .component_regions_modal(
            compose_frame(&app, area).plan.layers[0].rects[&Region::Surface(SurfaceId::Approval)],
        )
        .into_iter()
        .find(|(_, id)| {
            *id == HitId::Choice {
                question: 0,
                choice: 0,
            }
        })
        .unwrap()
        .0;
    assert_ne!(
        terminal.backend().buffer()[(selected_rect.x, selected_rect.y)].bg,
        app.theme.bg_hover
    );
}
