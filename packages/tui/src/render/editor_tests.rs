use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use crate::{
    app::{AppState, HitId, InitialOptions},
    input::pointer::route_pointer,
    layout::compose_frame,
    navigation::Region,
};

use super::render;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-editor-render-test"),
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

fn composer_body_rows(terminal: &Terminal<TestBackend>, composer: Rect) -> Vec<String> {
    (composer.y + 1..composer.bottom() - 1)
        .map(|y| {
            (composer.x..composer.right() - 1)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect()
        })
        .collect()
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
fn editor_paints_scrollbar_after_content_exceeds_max_lines() {
    let mut app = app();
    app.editor
        .restore_text("one\ntwo\nthree\nfour\nfive\nsix\nseven");
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();
    let scrollbar_x = composer.right() - 1;
    let scrollbar_rows = (composer.y + 1..composer.bottom() - 1)
        .map(|y| &buffer[(scrollbar_x, y)])
        .collect::<Vec<_>>();

    assert_eq!(composer.height, 8);
    assert_eq!(buffer[(scrollbar_x, composer.y)].symbol(), "─");
    assert!(
        scrollbar_rows
            .iter()
            .any(|cell| cell.symbol() == "║" && cell.fg == app.theme.scrollbar_bg),
        "expected a scrollbar track"
    );
    assert!(
        scrollbar_rows
            .iter()
            .any(|cell| cell.symbol() == "█" && cell.fg == app.theme.scrollbar_fg),
        "expected a scrollbar thumb"
    );
    assert_eq!(
        scrollbar_rows.last().map(|cell| cell.symbol()),
        Some("█"),
        "the latest cursor position pins the scrollbar thumb to the bottom"
    );
}

#[test]
fn editor_reserves_scrollbar_gutter_before_overflow() {
    let mut app = app();
    let text = "a".repeat(79);
    app.editor.restore_text(&text);
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];
    let terminal = draw(&app, area);
    let buffer = terminal.backend().buffer();
    let gutter_x = composer.right() - 1;

    assert_eq!(composer.height, 3);
    assert_eq!(buffer[(gutter_x, composer.y + 1)].symbol(), " ");
}

#[test]
fn composer_wheel_scroll_moves_content_with_scrollbar_viewport() {
    let mut app = app();
    app.editor
        .restore_text("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten");
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];
    let before = draw(&app, area);
    let before_rows = composer_body_rows(&before, composer);

    route_pointer(
        &mut app,
        area,
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: composer.x + 1,
            row: composer.y + 1,
            modifiers: KeyModifiers::NONE,
        },
    );

    let after = draw(&app, area);
    let after_rows = composer_body_rows(&after, composer);
    assert!(before_rows[0].starts_with("five"), "{before_rows:?}");
    assert!(after_rows[0].starts_with("two"), "{after_rows:?}");
    assert_ne!(before_rows, after_rows);
}

#[test]
fn composer_gutter_click_jumps_to_the_selected_viewport_row() {
    let mut app = app();
    app.editor
        .restore_text("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten");
    let area = Rect::new(0, 0, 80, 24);
    let composer = compose_frame(&app, area).plan.rects[&Region::Composer];
    let gutter_x = composer.right() - 1;

    route_pointer(
        &mut app,
        area,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: gutter_x,
            row: composer.y + 1,
            modifiers: KeyModifiers::NONE,
        },
    );

    let after = draw(&app, area);
    let after_rows = composer_body_rows(&after, composer);
    assert!(after_rows[0].starts_with("one"), "{after_rows:?}");
}
