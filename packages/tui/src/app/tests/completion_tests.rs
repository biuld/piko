use super::*;

#[test]
fn completion_acceptance_replaces_range() {
    let mut app = app();
    app.editor.restore_text("/res");
    app.refresh_suggestions();
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/resume ");

    app.editor.restore_text("/res");
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
fn pasted_absolute_image_path_is_loaded_as_an_attachment_effect() {
    let mut app = app();
    let effects = app.dispatch(EditorAction::InsertPaste("/Users/test/My Image.PNG".into()).into());

    assert!(app.editor.is_empty(), "path must not become editor text");
    assert!(!app.editor.auto_complete.is_active());
    assert!(matches!(
        effects.as_slice(),
        [Effect::ReadImageFile { path, expected_draft: None }]
            if path == &std::path::PathBuf::from("/Users/test/My Image.PNG")
    ));
}

#[test]
fn pasted_escaped_space_absolute_image_path_is_unescaped() {
    let mut app = app();
    let effects =
        app.dispatch(EditorAction::InsertPaste("/Users/test/My\\ Image.png".into()).into());

    assert!(matches!(
        effects.as_slice(),
        [Effect::ReadImageFile { path, expected_draft: None }]
            if path == &std::path::PathBuf::from("/Users/test/My Image.png")
    ));
}

#[test]
fn finder_drag_key_sequence_replaces_path_with_image_placeholder() {
    let mut app = app();
    let path = "/Users/test/Finder_Drag.PNG";
    let mut final_effects = Vec::new();
    for ch in path.chars() {
        final_effects = app.dispatch(EditorAction::InsertChar(ch).into());
    }

    assert_eq!(app.editor.text(), path);
    assert!(matches!(
        final_effects.as_slice(),
        [Effect::ReadImageFile { path: effect_path, expected_draft: Some(expected) }]
            if effect_path == &std::path::PathBuf::from(path) && expected == path
    ));

    app.dispatch(
        EditorAction::ReplaceDraftWithImage {
            expected_text: path.into(),
            filename: "Finder_Drag.PNG".into(),
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }
        .into(),
    );
    assert_eq!(app.editor.text(), "[Image #1: Finder_Drag.PNG]");
    assert!(!app.editor.auto_complete.is_active());
}

#[test]
fn stale_image_read_does_not_replace_changed_draft() {
    let mut app = app();
    app.editor.restore_text("new input");
    app.dispatch(
        EditorAction::ReplaceDraftWithImage {
            expected_text: "/Users/test/old.png".into(),
            filename: "old.png".into(),
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }
        .into(),
    );
    assert_eq!(app.editor.text(), "new input");
}

#[test]
fn unsupported_or_relative_pasted_paths_remain_text() {
    let mut app = app();
    assert!(
        app.dispatch(EditorAction::InsertPaste("notes/image.png".into()).into())
            .is_empty()
    );
    assert_eq!(app.editor.text(), "notes/image.png");

    app.editor.restore_text("");
    assert!(
        app.dispatch(EditorAction::InsertPaste("/Users/test/notes.txt".into()).into())
            .is_empty()
    );
    assert_eq!(app.editor.text(), "/Users/test/notes.txt");
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

    // Type a prefix that uniquely matches /quit.
    app.editor.restore_text("/qui");
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
                submit_on_accept: false,
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
                submit_on_accept: false,
            },
        ]);
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Get raw text (which expands references and takes the text)
    let submitted = app.editor.take_trimmed().unwrap();
    assert_eq!(submitted, "@src/main.rs");
}

#[test]
fn enter_accepts_file_without_submitting_chat() {
    let mut app = live_app();
    app.editor.auto_complete.active = true;
    app.editor.auto_complete.list =
        crate::ui::components::selectable_list::SelectableList::new(vec![
            crate::features::auto_completion::CompletionRow {
                replacement: "@src/main.rs ".to_string(),
                start: 0,
                end: 0,
                cells: vec![],
                keep_active: false,
                submit_on_accept: false,
            },
        ]);
    let effects = app.dispatch(EditorAction::AcceptAndSubmitSuggestion.into());
    assert!(effects.is_empty());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");
}

#[test]
fn enter_completes_argument_command_without_running_it() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "session.rename".into(),
                title: "Rename session".into(),
                detail: "Set a new name".into(),
                invoke: HostCommandInvoke::Args { schema: Vec::new() },
                group: Some(HostCommandGroup::Session),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    app.editor.restore_text("/rena");
    app.refresh_suggestions();
    let effects = app.dispatch(EditorAction::AcceptAndSubmitSuggestion.into());
    assert!(effects.is_empty());
    assert_eq!(app.editor.text(), "/rename ");
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
