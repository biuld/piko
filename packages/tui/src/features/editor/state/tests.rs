use super::*;

fn editor_with_history() -> Editor {
    let mut editor = Editor::default();
    editor.restore_text("first");
    assert_eq!(editor.take_trimmed().as_deref(), Some("first"));
    editor.restore_text("second");
    assert_eq!(editor.take_trimmed().as_deref(), Some("second"));
    editor
}

#[test]
fn replace_range_replaces_existing_text() {
    let mut editor = Editor::default();
    editor.restore_text("/res");
    editor.replace_range(0, 4, "/resume");
    assert_eq!(editor.text(), "/resume");
}

#[test]
fn history_restores_live_draft_after_newest() {
    let mut editor = editor_with_history();
    editor.restore_text("draft");
    editor.history_prev();
    assert_eq!(editor.text(), "second");
    editor.history_next();
    assert_eq!(editor.text(), "draft");
}

#[test]
fn large_paste_expands_on_submit() {
    let config = EditorConfig::default();
    let mut editor = Editor::default();
    let paste = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk";
    editor.insert_paste(paste, &config);
    assert_eq!(editor.text(), "[paste #1 +11 lines]");
    assert_eq!(editor.take_trimmed().as_deref(), Some(paste));
    assert!(editor.text().is_empty());
}

#[test]
fn cursor_offset_handles_multibyte_text() {
    let mut editor = Editor::default();
    editor.restore_text("你好");
    assert_eq!(editor.cursor(), "你好".len());
}

#[test]
fn cursor_screen_col_uses_terminal_width() {
    let mut editor = Editor::default();
    editor.restore_text("是的");
    assert_eq!(editor.cursor_line_col(80, 6), (0, 4));
}

#[test]
fn cursor_movement_is_bounded_by_line() {
    let mut editor = Editor::default();
    editor.restore_text("a\nb");
    editor.move_line_start();
    editor.move_left();
    assert_eq!(editor.cursor(), 2);
    editor.move_right();
    assert_eq!(editor.cursor(), 3);
}

#[test]
fn visible_height_grows_with_lines() {
    let mut editor = Editor::default();
    editor.restore_text("a\nb\nc");
    assert_eq!(editor.visible_height(&EditorConfig::default(), 80), 5);
}

#[test]
fn visible_height_grows_with_wrapped_visual_lines() {
    let mut editor = Editor::default();
    editor.restore_text("abcd");
    assert_eq!(editor.visible_height(&EditorConfig::default(), 2), 4);
    assert_eq!(editor.cursor_line_col(2, 2), (1, 2));
}

#[test]
fn cursor_row_stays_inside_visible_window_when_content_exceeds_max_lines() {
    let mut editor = Editor::default();
    editor.restore_text("a\nb\nc\nd\ne\nf\ng");
    assert_eq!(editor.visible_height(&EditorConfig::default(), 80), 8);
    assert_eq!(editor.cursor_line_col(80, 6), (5, 1));
}
