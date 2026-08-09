use super::*;
use crate::domain::tools::call::ToolCall;
use piko_orchd_api::tools::ToolExecutionContext;
use std::path::PathBuf;

fn policy() -> EffectivePermissions {
    EffectivePermissions {
        version: 1,
        read_roots: vec![PathBuf::from(".")],
        write_roots: vec![PathBuf::from(".")],
        scratch_roots: vec![],
        denied_read_roots: vec![PathBuf::from(".git"), PathBuf::from(".piko")],
        denied_write_roots: vec![],
        network: false.into(),
    }
}

fn context() -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: "session".into(),
        agent_instance_id: "agent".into(),
        execution_id: "exec".into(),
        cancellation: None,
        agent_id: "root".into(),
        agent_role: None,
        tool_set_ids: vec![],
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: Some(0),
        tool_entity_id: Some("entity".into()),
        host_context: None,
        source_turn_id: None,
        context_remaining: None,
    }
}

fn call(name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments: args,
        partial_json: None,
    }
}

#[tokio::test]
async fn edit_applies_unique_replacement() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.rs"), "fn one() {}\nfn two() {}\n").unwrap();

    let result = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "edit",
            serde_json::json!({
                "path": "a.rs",
                "edits": [{ "oldText": "fn one() {}", "newText": "fn renamed() {}" }]
            }),
        ),
        &context(),
    )
    .await;
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.rs")).unwrap(),
        "fn renamed() {}\nfn two() {}\n"
    );
    let change = result
        .value
        .as_ref()
        .and_then(|value| value.get(FILE_CHANGE_DETAILS_KEY))
        .expect("exact file change details");
    assert_eq!(change["path"], "a.rs");
    assert_eq!(change["before"], "fn one() {}\nfn two() {}\n");
    assert_eq!(change["after"], "fn renamed() {}\nfn two() {}\n");
}

#[tokio::test]
async fn edit_rejects_empty_old_text() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.rs"), "fn one() {}\n").unwrap();

    let result = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "edit",
            serde_json::json!({
                "path": "a.rs",
                "edits": [{ "oldText": "", "newText": "x" }]
            }),
        ),
        &context(),
    )
    .await;
    let error = result.error.expect("empty oldText must fail");
    assert_eq!(error.code, "edit_requires_old_text");
    assert!(!result.ok);
}

#[tokio::test]
async fn edit_rejects_non_unique_match_with_line_numbers() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("a.rs"),
        "let x = 1;\nlet y = 2;\nlet x = 3;\n",
    )
    .unwrap();

    let result = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "edit",
            serde_json::json!({
                "path": "a.rs",
                "edits": [{ "oldText": "let x =", "newText": "let z =" }]
            }),
        ),
        &context(),
    )
    .await;
    let error = result.error.expect("non-unique match must fail");
    assert_eq!(error.code, "edit_not_unique");
    assert!(error.message.contains("2 times"));
    assert!(error.message.contains("at lines 1, 3"));
    assert!(!result.ok);
}

#[tokio::test]
async fn edit_not_found_message_guides_the_model() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.rs"), "fn one() {}\n").unwrap();

    let result = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "edit",
            serde_json::json!({
                "path": "a.rs",
                "edits": [{ "oldText": "fn missing() {}", "newText": "fn x() {}" }]
            }),
        ),
        &context(),
    )
    .await;
    let error = result.error.expect("missing oldText must fail");
    assert_eq!(error.code, "edit_not_found");
    assert!(error.message.contains("Read the file"));
    assert!(!result.ok);
}

#[tokio::test]
async fn write_captures_exact_create_and_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let create = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "write",
            serde_json::json!({ "path": "a.txt", "content": "one\n" }),
        ),
        &context(),
    )
    .await;
    let create_change = &create.value.unwrap()[FILE_CHANGE_DETAILS_KEY];
    assert!(create_change["before"].is_null());
    assert_eq!(create_change["after"], "one\n");

    let overwrite = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "write",
            serde_json::json!({ "path": "./a.txt", "content": "two" }),
        ),
        &context(),
    )
    .await;
    let overwrite_change = &overwrite.value.unwrap()[FILE_CHANGE_DETAILS_KEY];
    assert_eq!(overwrite_change["path"], "a.txt");
    assert_eq!(overwrite_change["before"], "one\n");
    assert_eq!(overwrite_change["after"], "two");
}

#[tokio::test]
async fn write_and_edit_are_denied_inside_dot_piko() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".piko")).unwrap();
    std::fs::write(temp.path().join(".piko/approvals.json"), "{}").unwrap();

    let write = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "write",
            serde_json::json!({
                "path": ".piko/approvals.json",
                "content": r#"{"fingerprints":{"bash:git":{"tool_name":"bash"}}}"#
            }),
        ),
        &context(),
    )
    .await;
    let error = write.error.expect(".piko write must be denied");
    assert_eq!(error.code, "access_denied");
    assert!(!write.ok);
    assert_eq!(
        std::fs::read_to_string(temp.path().join(".piko/approvals.json")).unwrap(),
        "{}",
        "approvals file must remain untouched"
    );

    let edit = execute_workspace_tool(
        temp.path(),
        &policy(),
        &call(
            "edit",
            serde_json::json!({
                "path": ".piko/approvals.json",
                "edits": [{ "oldText": "{}", "newText": "{\"granted\":true}" }]
            }),
        ),
        &context(),
    )
    .await;
    let error = edit.error.expect(".piko edit must be denied");
    assert_eq!(error.code, "access_denied");
    assert!(!edit.ok);
}
