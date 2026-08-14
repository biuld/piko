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
fn empty_stream_paints_welcome_banner() {
    let app = app();
    let area = Rect::new(0, 0, 80, 24);
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    assert!(
        text.contains("piko"),
        "expected welcome title/logo in empty stream:\n{text}"
    );
    assert!(
        text.contains("coding agent"),
        "expected tagline in empty stream:\n{text}"
    );
    assert!(
        text.contains("submit prompt"),
        "expected tip line in empty stream:\n{text}"
    );
    // Bordered card uses box-drawing corners.
    assert!(
        text.contains('┌') && text.contains('└'),
        "expected welcome card border:\n{text}"
    );
}

#[test]
fn idle_guidance_row_paints_composer_hints_above_composer() {
    let app = app();
    let area = Rect::new(0, 0, 80, 24);
    let composed = compose_frame(&app, area);
    let guidance = composed.plan.rects[&Region::Guidance];
    let composer = composed.plan.rects[&Region::Composer];
    let terminal = draw(&app, area);
    let text = (guidance.x..guidance.right())
        .map(|x| terminal.backend().buffer()[(x, guidance.y)].symbol())
        .collect::<String>();

    assert_eq!(guidance.y + 1, composer.y);
    assert!(text.contains("/ commands · @ files · Enter send"), "{text}");
}

#[test]
fn dock_stack_paints_muted_boundary_below_stream() {
    let app = app();
    let area = Rect::new(0, 0, 80, 24);
    let composed = compose_frame(&app, area);
    let stream = composed.plan.rects[&Region::Stream];
    let boundary = composed.plan.rects[&Region::DockBoundary];
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();

    assert_eq!(stream.bottom(), boundary.y);
    assert_eq!(boundary.height, 1);
    assert_eq!(buffer[(boundary.x, boundary.y)].symbol(), "─");
    assert_eq!(buffer[(boundary.x, boundary.y)].fg, app.theme.border_muted);
}

#[test]
fn topmost_suggest_shares_dock_boundary_title_without_a_second_top_rule() {
    let mut app = app();
    app.editor.insert_char('/');
    app.refresh_suggestions();
    let area = Rect::new(0, 0, 80, 24);
    let composed = compose_frame(&app, area);
    let boundary = composed.plan.rects[&Region::DockBoundary];
    let suggest = composed.plan.rects[&Region::Suggest];
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();
    let boundary_text = (boundary.x..boundary.right())
        .map(|x| buffer[(x, boundary.y)].symbol())
        .collect::<String>();
    let first_suggest_row = (suggest.x..suggest.right())
        .map(|x| buffer[(x, suggest.y)].symbol())
        .collect::<String>();

    assert_eq!(boundary.bottom(), suggest.y);
    assert!(boundary_text.contains("slash commands"), "{boundary_text}");
    assert!(first_suggest_row.contains("/resume"), "{first_suggest_row}");
    assert!(!first_suggest_row.contains("slash commands"));
}

#[test]
fn select_surface_projects_its_hint_in_guidance_above_the_surface() {
    let mut app = app();
    app.push_surface(SurfaceId::Agents);
    let area = Rect::new(0, 0, 80, 24);
    let composed = compose_frame(&app, area);
    let layer = &composed.plan.layers[0];
    let guidance = layer.rects[&Region::Guidance];
    let surface = layer.rects[&Region::Surface(SurfaceId::Agents)];
    let terminal = draw(&app, area);
    let text = (guidance.x..guidance.right())
        .map(|x| terminal.backend().buffer()[(x, guidance.y)].symbol())
        .collect::<String>();

    assert_eq!(guidance.y + 1, surface.y);
    assert!(
        text.contains("↑/↓ navigate · Enter confirm · Esc cancel"),
        "{text}"
    );
}

#[test]
fn notice_replaces_hint_without_moving_guidance() {
    let mut app = app();
    let area = Rect::new(0, 0, 80, 24);
    let before = compose_frame(&app, area).plan.rects[&Region::Guidance];
    app.notifications
        .push(NotificationLevel::Warning, "needs attention");
    let after = compose_frame(&app, area).plan.rects[&Region::Guidance];
    let terminal = draw(&app, area);
    let text = (after.x..after.right())
        .map(|x| terminal.backend().buffer()[(x, after.y)].symbol())
        .collect::<String>();

    assert_eq!(before, after);
    assert!(text.contains("needs attention · F8 dismiss"), "{text}");
}

#[test]
fn notice_component_paints_its_hover_background() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.notifications
        .push(NotificationLevel::Warning, "click to dismiss");
    app.hovered = Some((Region::Guidance, Some(HitId::Notice)));
    let area = Rect::new(0, 0, 80, 24);
    let notice = compose_frame(&app, area).plan.rects[&Region::Guidance];

    let terminal = draw(&app, area);

    assert_eq!(
        terminal.backend().buffer()[(notice.x + 1, notice.y)].bg,
        app.theme.bg_hover
    );
}

#[test]
fn notice_row_uses_distinct_severity_glyphs_without_level_words() {
    for (level, glyph, level_word) in [
        (NotificationLevel::Info, "ⓘ", "info"),
        (NotificationLevel::Warning, "▲", "warning"),
        (NotificationLevel::Error, "✗", "error"),
    ] {
        let mut app = app();
        app.notifications.push(level, "notice body");
        let area = Rect::new(0, 0, 80, 24);
        let notice = compose_frame(&app, area).plan.rects[&Region::Guidance];
        let terminal = draw(&app, area);
        let text = (notice.x..notice.right())
            .map(|x| terminal.backend().buffer()[(x, notice.y)].symbol())
            .collect::<String>();

        assert!(text.contains(&format!("{glyph}  notice body")), "{text}");
        assert!(!text.contains(level_word), "{text}");
    }
}

#[test]
fn notification_panel_renders_metadata_then_wrapped_body() {
    let mut app = app();
    app.notifications.push(
        NotificationLevel::Error,
        "a notification body that remains separate from its status metadata",
    );
    app.notifications.open_modal();
    app.push_surface(SurfaceId::Notifications);
    let area = Rect::new(0, 0, 100, 30);
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();
    let rows = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let header = rows
        .iter()
        .position(|row| row.contains("✗ global · dismissible · active") && row.contains("[Copy]"))
        .expect("notification metadata header");

    assert!(
        rows[header + 1].contains("a notification body"),
        "{}",
        rows.join("\n")
    );
}

fn assert_editor_focus_feedback(mut app: AppState) {
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];

    let idle = draw(&app, area);
    // Composer has no elevated fill — body cells must not use bg_elevated.
    assert_ne!(
        idle.backend().buffer()[(composer.x + 1, composer.y + 1)].bg,
        app.theme.bg_elevated
    );
    assert_eq!(
        idle.backend().buffer()[(composer.x, composer.y)].fg,
        app.theme.prompt_border_active
    );

    app.hovered = Some((Region::Composer, Some(HitId::Composer)));
    let hovered = draw(&app, area);

    assert_ne!(
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
        selected_idx: 0,
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
