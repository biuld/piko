use super::model::{BodyLine, LineKind, ToolBody};
use super::*;

#[test]
fn read_uses_title_meta_and_code_view() {
    let args = r#"{"path":"src/main.rs","offset":10,"limit":20}"#;
    let result = r#"{"content":"fn main() {}\n","totalLines":12,"linesRead":1}"#;
    let presented = present_tool("read", args, Some(result), None);
    let meta = presented.title_meta.expect("title meta");
    assert!(meta.contains("src/main.rs"), "{meta}");
    assert!(meta.contains('L') || meta.contains("lines"), "{meta}");
    match presented.body {
        ToolBody::Code(code) => {
            assert_eq!(code.start_line, 10);
            assert!(code.lines.iter().any(|l| l.contains("fn main")));
        }
        other => panic!("expected code view, got {other:?}"),
    }
}

#[test]
fn exec_command_title_and_terminal_body() {
    let args = r#"{"cmd":"cargo test -p piko-tui"}"#;
    let result = r#"{"state":"exited","exit_code":0,"output":"ok\n1 passed\n"}"#;
    let presented = present_tool("exec_command", args, Some(result), None);
    let meta = presented.title_meta.as_deref().expect("title");
    assert!(
        meta.starts_with("$ cargo test"),
        "title meta should be the command: {meta}"
    );
    assert!(
        !meta.contains("exit"),
        "exit code belongs on the right badge, not title meta: {meta}"
    );
    let badge = presented.title_badge.as_ref().expect("exit badge");
    assert_eq!(badge.text, "exit 0");
    assert_eq!(badge.tone, BadgeTone::Success);
    let plain = presented.plain_body_lines().join("\n");
    assert!(plain.contains("$ cargo test"), "{plain}");
    assert!(plain.contains("1 passed"), "{plain}");
    assert!(
        !plain.contains("exit 0") && !plain.contains("✓ 0"),
        "body should not repeat exit status: {plain}"
    );
    assert!(!plain.starts_with('{'), "{plain}");
}

#[test]
fn exec_command_nonzero_exit_uses_error_badge() {
    let args = r#"{"cmd":"false"}"#;
    let result = r#"{"state":"exited","exit_code":127,"output":"","wall_time_seconds":0.06}"#;
    let presented = present_tool("exec_command", args, Some(result), None);
    assert_eq!(presented.title_meta.as_deref(), Some("$ false"));
    let badge = presented.title_badge.as_ref().expect("badge");
    assert_eq!(badge.text, "exit 127");
    assert_eq!(badge.tone, BadgeTone::Error);
    assert_eq!(badge.duration.as_deref(), Some("60ms"));
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        !plain.contains("time") && !plain.contains("60ms"),
        "duration should live on the title, not the body: {plain}"
    );
}

#[test]
fn todo_write_checklist_with_progress() {
    let args = r#"{"todos":[{"id":1,"status":"completed","content":"done task"},{"id":2,"status":"pending","content":"later"}]}"#;
    let presented = present_tool("todo_write", args, None, None);
    assert_eq!(presented.title_meta.as_deref(), Some("1/2 done"));
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains('✓') && plain.contains("done task"),
        "{plain}"
    );
    assert!(plain.contains('·') && plain.contains("later"), "{plain}");
    // No serial ids in the checklist body.
    assert!(!plain.contains("✓ 1") && !plain.contains("· 2"), "{plain}");
}

#[test]
fn unknown_tool_uses_primary_field_as_title() {
    let args = r#"{"query":"hello","limit":3}"#;
    let presented = present_tool("search_web", args, None, None);
    assert_eq!(presented.title_meta.as_deref(), Some("hello"));
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("query") || plain.contains("hello"),
        "{plain}"
    );
}

#[test]
fn edit_args_render_as_ide_diff_without_edit_hunk_headers() {
    let args = r#"{"path":"src/lib.rs","edits":[{"oldText":"fn a() {}\n","newText":"fn a() {\n  ok\n}\n"}]}"#;
    let presented = present_tool(
        "edit",
        args,
        Some(r#"{"edited":true,"editsApplied":1}"#),
        None,
    );
    let meta = presented.title_meta.expect("meta");
    assert!(meta.contains("src/lib.rs"), "{meta}");
    match presented.body {
        ToolBody::Diff(diff) => {
            assert!(
                !diff.stats.is_empty(),
                "expected +/− stats, got {:?}",
                diff.stats
            );
            assert!(
                diff.rows.iter().any(
                    |r| matches!(r, DiffRow::Delete { text, .. } if text.contains("fn a() {}"))
                ),
                "missing delete: {:?}",
                diff.rows
            );
            let plain = diff.to_plain_lines().join("\n");
            assert!(!plain.contains("@@ edit"), "{plain}");
        }
        other => panic!("expected diff, got {other:?}"),
    }
}

#[test]
fn edit_file_change_prefers_full_file_context_diff() {
    let details =
        r#"{"_pikoFileChange":{"path":"src/lib.rs","before":"a\nold\nb\n","after":"a\nnew\nb\n"}}"#;
    let presented = present_tool(
        "edit",
        r#"{"path":"src/lib.rs","edits":[{"oldText":"old","newText":"new"}]}"#,
        None,
        Some(details),
    );
    match presented.body {
        ToolBody::Diff(diff) => {
            assert_eq!(diff.path, "src/lib.rs");
            assert!(
                diff.rows
                    .iter()
                    .any(|r| matches!(r, DiffRow::Context { text, .. } if text == "a")),
                "missing context: {:?}",
                diff.rows
            );
            assert!(
                diff.rows
                    .iter()
                    .any(|r| matches!(r, DiffRow::Delete { text, .. } if text == "old")),
                "{:?}",
                diff.rows
            );
        }
        other => panic!("expected diff, got {other:?}"),
    }
}

#[test]
fn spawn_agent_uses_plain_title_and_meta_body() {
    let args = r#"{"agent_spec_id":"coder","prompt":"fix the tests"}"#;
    let result = r#"{
        "agent_instance_id":"agent_abc12345",
        "agent_spec_id":"coder",
        "attached":true,
        "outcome":{"type":"succeeded","usage":{
            "input":2424,"output":63,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
        }},
        "summary":"all green",
        "usage":{
            "input":2424,"output":63,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
        }
    }"#;
    let presented = present_tool("spawn_agent", args, Some(result), None);
    let meta = presented.title_meta.as_deref().expect("meta");
    assert!(
        meta.contains("coder") && meta.contains("fix the tests"),
        "plain title meta: {meta}"
    );
    assert!(
        !meta.contains('◆') && !meta.contains('●') && !meta.contains('▶'),
        "title must stay ASCII for width stability: {meta}"
    );
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("agent") && plain.contains("agent_abc12345"),
        "{plain}"
    );
    assert!(plain.contains("succeeded"), "outcome type only: {plain}");
    assert!(
        !plain.contains("cacheRead") && !plain.contains("\"type\""),
        "must not dump raw outcome JSON: {plain}"
    );
    assert!(plain.contains("all green"), "summary prose: {plain}");
    assert!(
        plain.contains("usage") && (plain.contains("2.4k") || plain.contains("2424")),
        "usage strip: {plain}"
    );
    // Finished spawn: prompt is on the title; body should not re-list args.
    assert!(
        !plain.contains("fix the tests"),
        "do not duplicate prompt when summary exists: {plain}"
    );
    assert!(
        !plain.contains('└') && !plain.contains('┌'),
        "no box art: {plain}"
    );
}

#[test]
fn spawn_detached_title_is_plain() {
    let args = r#"{"agent_spec_id":"scout","prompt":"explore"}"#;
    let presented = present_tool("spawn_agent_detached", args, None, None);
    let meta = presented.title_meta.expect("meta");
    assert!(
        meta.contains("detach") && meta.contains("scout"),
        "detached title: {meta}"
    );
    assert!(!meta.contains('▶') && !meta.contains('○'), "{meta}");
}

#[test]
fn list_agents_renders_plain_rows() {
    let result = r#"{
        "agents": [
            {"agent_instance_id":"root","agent_spec_id":"main","activity":"running","lifecycle":"open","parent_agent_instance_id":null},
            {"agent_instance_id":"child","agent_spec_id":"coder","activity":"idle","lifecycle":"open","parent_agent_instance_id":"root"}
        ]
    }"#;
    let presented = present_tool("list_agents", "{}", Some(result), None);
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("root") && plain.contains("[main]"),
        "{plain}"
    );
    assert!(
        plain.contains("child") && plain.contains("[coder]"),
        "{plain}"
    );
    assert!(
        !plain.contains('●') && !plain.contains('└') && !plain.contains('├'),
        "no tree art: {plain}"
    );
}

#[test]
fn list_agent_specs_body_is_card_blocks() {
    let result = r#"{
        "default_spawn_spec_id": "general",
        "specs": [
            {
                "id": "general",
                "name": "General",
                "role": "assistant",
                "description": "Default helper for everyday tasks."
            },
            {
                "id": "coder",
                "name": "coder",
                "role": "implementer",
                "description": "Writes and edits code."
            }
        ]
    }"#;
    let presented = present_tool("list_agent_specs", "{}", Some(result), None);
    assert_eq!(presented.title_meta.as_deref(), Some("2 specs"));
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("default") && plain.contains("general"),
        "{plain}"
    );
    assert!(
        plain.contains("(default)"),
        "mark default template: {plain}"
    );
    assert!(
        plain.contains("role") && plain.contains("assistant"),
        "{plain}"
    );
    assert!(
        plain.contains("Default helper"),
        "description prose: {plain}"
    );
    assert!(
        plain.contains("coder") && plain.contains("implementer"),
        "{plain}"
    );
    // name equal to id is omitted; distinct name is kept.
    assert!(
        plain.contains("name") && plain.contains("General"),
        "{plain}"
    );
    assert!(
        !plain.contains('└') && !plain.contains('◇') && !plain.contains('├'),
        "no tree / palette art: {plain}"
    );
}

#[test]
fn non_json_passthrough() {
    let presented = present_tool("custom", "not-json", Some("plain result"), None);
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("not-json") || plain.contains("plain result"),
        "{plain}"
    );
}

#[test]
fn request_user_input_maps_answers_not_raw_json() {
    let args = r#"{
      "title": "Ask User 功能测试",
      "questions": [
        {
          "id": "q1",
          "header": "基本信息",
          "prompt": "你喜欢用什么编程语言？",
          "choices": [
            {"id": "rust", "label": "Rust"},
            {"id": "python", "label": "Python"}
          ]
        },
        {
          "id": "q2",
          "header": "开发偏好",
          "prompt": "编辑器？",
          "choices": [
            {"id": "vim", "label": "Vim/Neovim"},
            {"id": "vscode", "label": "VS Code"}
          ]
        }
      ]
    }"#;
    let result = r#"{
      "answers": [
        {"questionId": "q1", "choiceId": "rust", "value": null},
        {"questionId": "q2", "choiceId": "vscode", "value": null}
      ]
    }"#;
    let presented = present_tool("request_user_input", args, Some(result), None);
    let plain = presented.plain_body_lines().join("\n");
    assert!(
        plain.contains("✓ Rust"),
        "selected choice marked in list: {plain}"
    );
    assert!(
        plain.contains("✓ VS Code"),
        "second selected choice marked: {plain}"
    );
    assert!(
        !plain.contains("answers")
            && !plain.contains("questionId")
            && !plain.contains("基本信息 ·"),
        "no redundant answers summary / raw JSON: {plain}"
    );
}

#[test]
fn body_line_kinds_cover_paint_paths() {
    let lines = [
        BodyLine::meta("cwd", "/tmp"),
        BodyLine::prompt("$ ls"),
        BodyLine::terminal("a.rs"),
        BodyLine::success("✓ 0"),
        BodyLine::todo(LineKind::TodoDone, "✓ 1  done"),
    ];
    assert_eq!(lines[0].to_plain(), "cwd  /tmp");
    assert!(matches!(
        lines[1],
        BodyLine::Text {
            kind: LineKind::Prompt,
            ..
        }
    ));
}
