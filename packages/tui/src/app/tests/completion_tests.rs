use super::*;

#[test]
fn completion_acceptance_replaces_range() {
    let mut app = app();
    app.editor.restore_text("/res");
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/res");

    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: test_command_catalog(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });
    app.refresh_suggestions();
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/resume ");
}

#[test]
fn test_completion_cycling_fills_editor() {
    let mut app = app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            // `/resume` and `/quit` are TUI-local commands, always merged in
            // regardless of what hostd advertises.
            commands: Vec::new(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });

    // Type "/q"
    app.editor.restore_text("/q");
    app.refresh_suggestions();

    // Check suggestions: should match /quit
    assert_eq!(app.editor.auto_complete.list.len(), 1);
    assert_eq!(app.editor.auto_complete.list.items[0].replacement, "/quit ");

    // Cycle next (Tab equivalent)
    app.dispatch(EditorAction::SuggestionSelectNext.into());
    // Editor should be updated automatically!
    assert_eq!(app.editor.text(), "/quit ");

    // Accept suggestion (Enter equivalent)
    app.dispatch(EditorAction::AcceptSuggestion.into());
    // Editor should remain "/quit "
    assert_eq!(app.editor.text(), "/quit ");
}

#[test]
fn test_file_completion_inserted_as_placeholder_block() {
    let mut app = app();

    // We mock file suggestions by manually updating AutoComplete state
    app.editor.auto_complete.active = true;
    app.editor.auto_complete.list =
        crate::ui::components::selectable_list::SelectableList::new(vec![
            crate::features::auto_completion::CompletionRow {
                replacement: "@src/main.rs ".to_string(),
                start: 0,
                end: 2,
                cells: vec![],
                keep_active: false,
            },
        ]);

    // Cycle next to preview
    app.dispatch(EditorAction::SuggestionSelectNext.into());
    // Editor should be filled with the placeholder "[@src/main.rs] "
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Accept suggestion
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Deleting the last character (the space)
    app.editor.backspace();
    assert_eq!(app.editor.text(), "[@src/main.rs]");

    // Deleting again should delete the ENTIRE placeholder block!
    app.editor.backspace();
    assert_eq!(app.editor.text(), "");

    // Re-do completion and submit to verify expansion
    app.editor.auto_complete.active = true;
    app.editor.auto_complete.list =
        crate::ui::components::selectable_list::SelectableList::new(vec![
            crate::features::auto_completion::CompletionRow {
                replacement: "@src/main.rs ".to_string(),
                start: 0,
                end: 0,
                cells: vec![],
                keep_active: false,
            },
        ]);
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Get raw text (which expands references and takes the text)
    let submitted = app.editor.take_trimmed().unwrap();
    assert_eq!(submitted, "@src/main.rs");
}

#[test]
fn ctrl_p_history_works_with_live_draft() {
    let mut app = app();
    app.editor.restore_text("first");
    app.dispatch(EditorAction::Submit.into());
    app.editor.restore_text("draft");

    app.dispatch(EditorAction::HistoryPrev.into());
    assert_eq!(app.editor.text(), "first");
    app.dispatch(EditorAction::HistoryNext.into());
    assert_eq!(app.editor.text(), "draft");
}

#[test]
fn slash_completion_visible_with_empty_results() {
    let mut app = app();
    app.editor.restore_text("/zzz");
    app.refresh_suggestions();
    assert!(app.has_suggestions());
    assert!(app.editor.auto_complete.list.is_empty());
}
