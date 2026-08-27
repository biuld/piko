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
fn mixed_image_submission_preserves_block_order() {
    let mut editor = Editor::default();
    editor.restore_text("  before ");
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    editor.insert_char(' ');
    editor.insert_paste("after  ", &EditorConfig::default());

    let submission = editor.take_submission().unwrap();
    assert_eq!(
        submission.content,
        piko_protocol::MessageContent::Blocks(vec![
            piko_protocol::ContentBlock::Text {
                text: "before ".into(),
            },
            piko_protocol::ContentBlock::Image {
                data: "AA==".into(),
                mime_type: "image/png".into(),
            },
            piko_protocol::ContentBlock::Text {
                text: " after".into(),
            },
        ])
    );
    assert!(editor.is_empty());
}

#[test]
fn image_only_submission_and_restore_are_supported() {
    let mut editor = Editor::default();
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    let content = editor.take_submission().unwrap().content;
    assert!(matches!(
        &content,
        piko_protocol::MessageContent::Blocks(blocks)
            if matches!(blocks.as_slice(), [piko_protocol::ContentBlock::Image { data, .. }] if data == "AA==")
    ));

    editor.restore_content(&content);
    assert!(editor.text().contains("restored.png"));
    assert_eq!(editor.take_submission().unwrap().content, content);
}

#[test]
fn deleting_image_placeholder_removes_attachment() {
    let mut editor = Editor::default();
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    editor.backspace();
    assert!(editor.is_empty());
    assert!(editor.take_submission().is_none());
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
    // One cell is permanently reserved for the possible scrollbar gutter.
    assert_eq!(editor.visible_height(&EditorConfig::default(), 2), 6);
    assert_eq!(editor.cursor_line_col(2, 2), (1, 1));
}

#[test]
fn cursor_row_stays_inside_visible_window_when_content_exceeds_max_lines() {
    let mut editor = Editor::default();
    editor.restore_text("a\nb\nc\nd\ne\nf\ng");
    assert_eq!(editor.visible_height(&EditorConfig::default(), 80), 8);
    assert_eq!(editor.cursor_line_col(80, 6), (5, 1));
}

#[test]
fn reference_cursor_movement_and_editing_preserve_atomic_payload() {
    let mut editor = Editor::default();
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    editor.move_left();
    assert_eq!(editor.cursor(), 0);
    editor.insert_char('x');

    let submission = editor.take_submission().unwrap();
    assert!(matches!(
        submission.content,
        piko_protocol::MessageContent::Blocks(ref blocks)
            if matches!(blocks.as_slice(), [
                piko_protocol::ContentBlock::Text { text },
                piko_protocol::ContentBlock::Image { data, .. }
            ] if text == "x" && data == "AA==")
    ));
}

#[test]
fn word_movement_never_stops_inside_a_reference() {
    let mut editor = Editor::default();
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    editor.insert_char(' ');
    editor.move_word_left();
    assert_eq!(editor.cursor(), 0);
    editor.move_word_right();
    assert_eq!(editor.cursor(), "[Image #1: clipboard.png]".len());
}

#[test]
fn edits_before_a_reference_keep_its_payload_position_in_sync() {
    let mut editor = Editor::default();
    editor.restore_text("ab");
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    editor.move_line_start();
    editor.delete();
    let submission = editor.take_submission().unwrap();
    assert!(matches!(
        submission.content,
        piko_protocol::MessageContent::Blocks(ref blocks)
            if matches!(blocks.as_slice(), [
                piko_protocol::ContentBlock::Text { text },
                piko_protocol::ContentBlock::Image { data, .. }
            ] if text == "b" && data == "AA==")
    ));
}

#[test]
fn history_recall_preserves_image_payload() {
    let mut editor = Editor::default();
    editor.insert_image("clipboard.png", "AA==".into(), "image/png".into());
    let original = editor.take_submission().unwrap().content;
    editor.history_prev();
    assert_eq!(editor.take_submission().unwrap().content, original);
}

#[test]
fn pointer_position_selects_the_clicked_visual_row() {
    let mut editor = Editor::default();
    editor.restore_text("one\ntwo");
    editor.move_to_position(20, 4, 1, 1);
    assert_eq!(editor.cursor(), 1);
}

#[test]
fn cursor_follow_keeps_a_clicked_line_visible_after_editing() {
    let mut editor = Editor::default();
    editor.restore_text("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten");
    editor.scroll_up(80, 6, 3);
    assert!(!editor.cursor_is_visible(80, 6));

    editor.move_to_position(80, 8, 0, 1);
    assert!(editor.cursor_is_visible(80, 6));
    editor.move_left();
    assert!(editor.cursor_is_visible(80, 6));
    assert_eq!(editor.viewport.top_offset(10, 6), 1);
}

#[test]
fn word_and_line_deletion_use_their_full_ranges() {
    let mut editor = Editor::default();
    editor.restore_text("one two three");
    editor.delete_word_backward();
    assert_eq!(editor.text(), "one two ");
    editor.delete_to_line_start();
    assert!(editor.is_empty());
}
