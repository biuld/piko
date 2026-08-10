use super::component_lines;
use crate::app::ToolStatus;
use crate::features::timeline::{
    AssistantMessageComponent, ComponentId, ContentBlock, Timeline, TimelineComponent,
    TimelineEntry, ToolEntry,
};
use crate::theme::Theme;
use piko_protocol::agent_runtime::RealtimeDelta;

fn tool_entry(id: &str, name: &str) -> ToolEntry {
    ToolEntry::new(
        id.into(),
        name.into(),
        ToolStatus::Completed,
        String::new(),
        Some(r#"{"ok":true}"#.into()),
        None,
    )
}

fn empty_tool_use_assistant(id: &str) -> TimelineComponent {
    TimelineComponent::Assistant(AssistantMessageComponent {
        id: ComponentId::MessageId(id.into()),
        blocks: vec![ContentBlock::Text(String::new())],
        stop_reason: Some("tool_use".into()),
        error_message: None,
        timestamp: None,
    })
}

fn user_with_ts(text: &str, ts: i64) -> TimelineComponent {
    use crate::features::timeline::UserMessageComponent;
    TimelineComponent::User(UserMessageComponent {
        id: ComponentId::MessageId("u1".into()),
        text: text.into(),
        timestamp: Some(ts),
    })
}

fn assistant_with_ts(text: &str, ts: i64) -> TimelineComponent {
    TimelineComponent::Assistant(AssistantMessageComponent {
        id: ComponentId::MessageId("a1".into()),
        blocks: vec![ContentBlock::Text(text.into())],
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: Some(ts),
    })
}

#[test]
fn user_and_assistant_timestamp_shares_first_line() {
    let theme = Theme::dark();
    let ts = 1_786_359_296_000i64; // fixed epoch ms
    let user = component_lines(&user_with_ts("hello", ts), true, false, &theme, 40);
    // pad · body(+ts) · pad — not a dedicated timestamp row.
    assert_eq!(user.len(), 3, "user rows: {user:?}");
    let first_body: String = user[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(first_body.contains("hello"), "{first_body}");
    assert!(
        first_body.chars().filter(|c| *c == ':').count() >= 1,
        "timestamp on same line as body: {first_body}"
    );

    let assistant = component_lines(&assistant_with_ts("world", ts), true, false, &theme, 40);
    let first: String = assistant[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(first.contains("world"), "{first}");
    assert!(
        first.chars().filter(|c| *c == ':').count() >= 1,
        "assistant timestamp on first body line: {first}"
    );
}

#[test]
fn assistant_markdown_survives_timestamp_chrome() {
    use ratatui::style::Modifier;

    let theme = Theme::dark();
    let ts = 1_786_359_296_000i64;
    let component = TimelineComponent::Assistant(AssistantMessageComponent {
        id: ComponentId::MessageId("a-md".into()),
        blocks: vec![ContentBlock::Text(
            "answer is **Rust** and a table:\n\n| a | b |\n| - | - |\n| 1 | 2 |\n".into(),
        )],
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: Some(ts),
    });
    let lines = component_lines(&component, true, false, &theme, 80);
    let plain: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !plain.contains("**"),
        "bold markers must not survive with timestamp: {plain}"
    );
    assert!(plain.contains("Rust"), "bold text content kept: {plain}");
    assert!(
        !plain.contains('|'),
        "table pipes must not survive with timestamp: {plain}"
    );
    assert!(
        plain.contains('─') || plain.contains('1'),
        "table body still rendered: {plain}"
    );
    let bold_rust = lines
        .iter()
        .flat_map(|l| &l.spans)
        .any(|s| s.content.contains("Rust") && s.style.add_modifier.contains(Modifier::BOLD));
    assert!(bold_rust, "Rust should stay bold under clock chrome");
    let has_clock = plain.chars().filter(|c| *c == ':').count() >= 1;
    assert!(has_clock, "timestamp still painted: {plain}");
}

#[test]
fn long_user_body_wraps_inside_left_column_not_under_timestamp() {
    let theme = Theme::dark();
    let ts = 1_786_359_296_000i64;
    // Narrow width forces soft-wrap; every body line must leave room for the clock.
    let width = 24u16;
    let text = "abcdefghijklmnopqrstuvwxyz0123456789"; // longer than left column
    let lines = component_lines(&user_with_ts(text, ts), true, false, &theme, width);
    // Skip top/bottom pad.
    let body: Vec<String> = lines[1..lines.len() - 1]
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert!(body.len() >= 2, "expected wrap: {body:?}");
    // Clock only on first body line.
    assert!(
        body[0].chars().filter(|c| *c == ':').count() >= 1,
        "ts on first: {}",
        body[0]
    );
    for (i, row) in body.iter().enumerate().skip(1) {
        assert!(
            !row.contains(':') || !row.chars().any(|c| c.is_ascii_digit()),
            "continuation must not repaint clock: line {i}={row}"
        );
    }
    // Full row width is filled (left + reserved right), no black hole.
    for row in &body {
        let w: usize = row
            .chars()
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        // filled with spaces to width
        assert!(
            w >= usize::from(width).saturating_sub(1),
            "row under-filled: w={w} width={width} row={row:?}"
        );
    }
}

#[test]
fn collapsed_todo_tool_does_not_force_checklist_body() {
    use crate::app::ToolStatus;
    use crate::features::timeline::ToolEntry;

    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-todo".into(),
        "todo_write".into(),
        ToolStatus::Completed,
        r#"{"todos":[{"id":1,"status":"completed","content":"done task"},{"id":2,"status":"in_progress","content":"doing now"}]}"#.into(),
        Some(r#"{"updated":true}"#.into()),
        None,
    );
    // Strip is live truth — collapsed audit cards stay collapsed.
    tool.expanded = false;
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 48);
    let plain: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plain.contains("todo_write") || plain.contains("1/2 done"),
        "title remains: {plain}"
    );
    assert!(
        !plain.contains("done task"),
        "checklist body must not force-open: {plain}"
    );
}

#[test]
fn expanded_todo_tool_still_formats_checklist_history() {
    use crate::app::ToolStatus;
    use crate::features::timeline::ToolEntry;

    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-todo".into(),
        "todo_write".into(),
        ToolStatus::Completed,
        r#"{"todos":[{"id":1,"status":"completed","content":"done task"},{"id":2,"status":"in_progress","content":"doing now"}]}"#.into(),
        Some(r#"{"todos":[{"id":"1","status":"completed","content":"done task"},{"id":"2","status":"in_progress","content":"doing now"}]}"#.into()),
        None,
    );
    tool.expanded = true;
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 48);
    let plain: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(plain.contains("done task"), "history body: {plain}");
    assert!(plain.contains("doing now"), "history body: {plain}");
}

#[test]
fn missing_timestamp_skips_chrome() {
    let theme = Theme::dark();
    let user = component_lines(
        &TimelineComponent::User(crate::features::timeline::UserMessageComponent {
            id: ComponentId::MessageId("u".into()),
            text: "hi".into(),
            timestamp: None,
        }),
        true,
        false,
        &theme,
        32,
    );
    // Only pad + body + pad (3 lines) — no timestamp row.
    assert_eq!(user.len(), 3, "{user:?}");
}

#[test]
fn zero_height_assistant_does_not_double_gap_between_tools() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    timeline.push(TimelineEntry::Tool(tool_entry("c1", "list_agent_specs")));
    timeline
        .components
        .push_back(empty_tool_use_assistant("a-empty"));
    timeline.push(TimelineEntry::Tool(tool_entry("c2", "spawn_agent")));

    let lines = timeline.render_lines(&theme, 40);
    let blank_rows = lines
        .iter()
        .filter(|line| {
            line.spans
                .iter()
                .all(|span| span.content.chars().all(char::is_whitespace))
                && line.spans.iter().all(|span| span.style.bg.is_none())
        })
        .count();

    assert_eq!(blank_rows, 1, "unexpected blank rows: {lines:?}");
    assert!(
        lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("list_agent_specs"))
        }),
        "first tool missing: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("spawn_agent"))
        }),
        "second tool missing: {lines:?}"
    );
}

#[test]
fn streaming_chunks_render_as_coalesced_ordered_blocks() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    for (seq, delta) in [
        (
            0,
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ),
        (
            1,
            RealtimeDelta::Thinking {
                content_index: 0,
                delta: "thinking".into(),
            },
        ),
        (
            2,
            RealtimeDelta::Thinking {
                content_index: 0,
                delta: " now".into(),
            },
        ),
        (
            3,
            RealtimeDelta::Text {
                content_index: 0,
                delta: "hello".into(),
            },
        ),
        (
            4,
            RealtimeDelta::Text {
                content_index: 0,
                delta: " world".into(),
            },
        ),
    ] {
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s".into()),
            Some("root".into()),
            "msg",
            Some(seq),
            &delta,
        )
        .pop()
        .unwrap();
        timeline.apply_stream_item(&patch);
    }

    let rendered: Vec<String> = timeline
        .render_lines(&theme, 80)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    assert_eq!(rendered, [" thinking now", "", " hello world"]);
}

#[test]
fn tool_disclosure_and_hover_preserve_status_card_semantics() {
    let theme = Theme::dark();
    let mut tool = tool_entry("c1", "bash");
    let collapsed = component_lines(
        &TimelineComponent::Tool(tool.clone()),
        true,
        true,
        &theme,
        40,
    );
    // Collapsed is always 3 rows: pad · title · pad (or pad · title · preview).
    assert_eq!(collapsed.len(), 3, "collapsed height: {collapsed:?}");
    let title_text: String = collapsed[1]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        title_text.contains('>') || title_text.contains("bash"),
        "collapsed disclosure missing: {title_text}"
    );
    assert_eq!(collapsed[1].spans[0].style.fg, Some(theme.accent));
    assert_eq!(collapsed[1].spans[0].style.bg, Some(theme.tool_success_bg));

    tool.expanded = true;
    let expanded = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 40);
    let expanded_title: String = expanded[1]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        expanded_title.contains('▾') || expanded_title.contains("bash"),
        "expanded disclosure missing: {expanded_title}"
    );
    assert!(expanded.len() > collapsed.len());
}

#[test]
fn edit_tool_block_renders_ide_diff_with_gutter_and_change_colors() {
    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-edit".into(),
        "edit".into(),
        ToolStatus::Completed,
        r#"{"path":"a.rs","edits":[{"oldText":"x","newText":"y"}]}"#.into(),
        Some(r#"{"edited":true,"editsApplied":1}"#.into()),
        None,
    );
    tool.expanded = true;
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 48);
    let mut saw_delete = false;
    let mut saw_insert = false;
    let mut saw_title_with_path = false;
    for line in &lines {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if text.contains("edit") && text.contains("a.rs") {
            saw_title_with_path = true;
        }
        assert!(
            !text.contains("@@ edit"),
            "should not show @@ edit headers: {text}"
        );
        if text.contains('−') && text.contains('x') {
            saw_delete = true;
            assert!(
                line.spans
                    .iter()
                    .any(|s| s.style.fg == Some(theme.diff_delete_fg)),
                "delete should use delete fg: {line:?}"
            );
            assert!(
                line.spans
                    .iter()
                    .any(|s| s.style.bg == Some(theme.diff_delete_bg)),
                "delete should use delete bg: {line:?}"
            );
        }
        if text.contains('+') && text.contains('y') {
            saw_insert = true;
            assert!(
                line.spans
                    .iter()
                    .any(|s| s.style.fg == Some(theme.diff_insert_fg)),
                "insert should use insert fg: {line:?}"
            );
            assert!(
                line.spans
                    .iter()
                    .any(|s| s.style.bg == Some(theme.diff_insert_bg)),
                "insert should use insert bg: {line:?}"
            );
        }
    }
    assert!(
        saw_title_with_path,
        "tool name and path should share the title row: {lines:?}"
    );
    assert!(saw_delete, "delete line missing: {lines:?}");
    assert!(saw_insert, "insert line missing: {lines:?}");
}

#[test]
fn title_row_has_status_glyph_and_token_estimate_on_the_right() {
    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "call-abcdef01".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"a.rs"}"#.into(),
        // 20 chars → ceil(20/4) = 5 tokens under the hostd heuristic.
        Some("abcdefghijklmnopqrst".into()),
        Some("msg-parent99".into()),
    );

    // Collapsed is a fixed 3-line card.
    let collapsed = component_lines(
        &TimelineComponent::Tool(tool.clone()),
        true,
        false,
        &theme,
        72,
    );
    assert_eq!(
        collapsed.len(),
        3,
        "collapsed should be 3 lines: {collapsed:?}"
    );
    let title: String = collapsed[1]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(title.contains("read") && title.contains("a.rs"), "{title}");
    assert!(title.contains('✓'), "status glyph on title: {title}");
    assert!(title.contains('▸'), "collapsed disclosure glyph: {title}");
    assert!(title.contains("~5"), "token estimate after status: {title}");
    assert!(
        !title.contains("call-abc") && !title.contains("msg-pare"),
        "call/parent ids must not appear: {title}"
    );

    tool.expanded = true;
    let expanded = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 72);
    let expanded_title: String = expanded[1]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        expanded_title.contains('✓')
            && expanded_title.contains('▾')
            && expanded_title.contains("~5"),
        "expanded title keeps status+tokens: {expanded_title}"
    );
    let joined: String = expanded
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("call-abc") && !joined.contains("msg-pare"),
        "call/parent must not appear when expanded either: {joined}"
    );
}

#[test]
fn title_right_zone_keeps_token_unit_when_left_meta_is_long() {
    let theme = Theme::dark();
    // Long command meta must not steal the right zone or drop "~1.2k".
    let long_cmd = "x".repeat(80);
    let tool = ToolEntry::new(
        "c1".into(),
        "exec_command".into(),
        ToolStatus::Completed,
        format!(r#"{{"cmd":"{long_cmd}"}}"#),
        // exit + duration + large output → right cluster is wide
        Some(format!(
            r#"{{"state":"exited","exit_code":127,"wall_time_seconds":0.06,"output":"{}"}}"#,
            "y".repeat(4800)
        )),
        None,
    );
    for width in [40u16, 48, 56, 64, 80] {
        let lines = component_lines(
            &TimelineComponent::Tool(tool.clone()),
            true,
            false,
            &theme,
            width,
        );
        let title: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            title.contains("~1.2k"),
            "width={width}: full token estimate with unit 'k' must remain: {title}"
        );
        // At comfortable widths keep the full right cluster.
        if width >= 48 {
            assert!(
                title.contains("exit 127"),
                "width={width}: exit badge must remain: {title}"
            );
            assert!(
                title.contains("60ms"),
                "width={width}: duration must remain: {title}"
            );
            // Spacer: left content and "exit" must not be adjacent without blanks.
            if let Some(exit_at) = title.find("exit 127") {
                let before = &title[..exit_at];
                assert!(
                    before.ends_with("   ") || before.chars().rev().take(3).all(|c| c == ' '),
                    "width={width}: need ≥3-col spacer before right zone: {title:?}"
                );
            }
        }
        // Chip sep is middot glyph; no pipe / ellipsis.
        assert!(
            title.contains('·') && !title.contains('|') && !title.contains('…'),
            "width={width}: expect middot chip sep, no pipe: {title}"
        );
    }
}

#[test]
fn title_uses_embedded_usage_tokens_over_size_heuristic() {
    let theme = Theme::dark();
    // Real spawn-shaped result: usage.input+output dominates a short JSON body.
    let result = r#"{
        "agent_instance_id":"agent_spawn_abc",
        "outcome":{"type":"succeeded","usage":{
            "input":2424,"output":63,"cacheRead":0,"cacheWrite":0,
            "totalTokens":0,
            "cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}
        }},
        "summary":"hi",
        "usage":{
            "input":2424,"output":63,"cacheRead":0,"cacheWrite":0,
            "totalTokens":0,
            "cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}
        }
    }"#;
    let tool = ToolEntry::new(
        "c1".into(),
        "spawn_agent".into(),
        ToolStatus::Completed,
        r#"{"agent_spec_id":"general","prompt":"hello"}"#.into(),
        Some(result.into()),
        None,
    );
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 80);
    let title: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    // 2424+63 = 2487 → format_tokens → "2.5k"
    assert!(
        title.contains("~2.5k"),
        "title must use per-tool usage (not chars/4 size): {title}"
    );
    assert!(
        !title.contains("~326") && !title.contains("~3"),
        "must not show size heuristic when usage is present: {title}"
    );
}

#[test]
fn title_prefers_total_tokens_when_set() {
    let theme = Theme::dark();
    let result = r#"{"usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"totalTokens":1500,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}}"#;
    let tool = ToolEntry::new(
        "c1".into(),
        "spawn_agent".into(),
        ToolStatus::Completed,
        r#"{"prompt":"x"}"#.into(),
        Some(result.into()),
        None,
    );
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 64);
    let title: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        title.contains("~1.5k"),
        "totalTokens wins over input+output: {title}"
    );
}

#[test]
fn write_stdin_poll_title_has_no_ellipsis_glyph() {
    let theme = Theme::dark();
    let tool = ToolEntry::new(
        "c1".into(),
        "write_stdin".into(),
        ToolStatus::Completed,
        r#"{"session_id":"proc-22"}"#.into(),
        Some(r#"{"state":"exited","exit_code":0,"wall_time_seconds":2.2,"output":"ok"}"#.into()),
        None,
    );
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 64);
    let title: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        title.contains("poll proc-22") || title.contains("write_stdin"),
        "poll label expected: {title}"
    );
    assert!(
        !title.contains('…'),
        "must not use U+2026 ellipsis as a poll icon: {title}"
    );
    assert!(
        title.contains("exit 0") && title.contains("2.20s"),
        "right zone expected: {title}"
    );
}

#[test]
fn read_tool_title_carries_path_and_body_has_gutter() {
    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-read".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"Cargo.toml","offset":1}"#.into(),
        Some(r#"{"content":"[package]\nname = \"piko\"\n","totalLines":2,"linesRead":2}"#.into()),
        None,
    );
    tool.expanded = true;
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 60);
    let joined: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("read") && joined.contains("Cargo.toml"),
        "title should include path: {joined}"
    );
    assert!(
        joined.contains('│') || joined.contains("[package]"),
        "code body expected: {joined}"
    );
    assert!(
        !joined.contains(r#"{"path""#),
        "should not dump raw JSON: {joined}"
    );
}

#[test]
fn exec_tool_shows_command_left_and_exit_badge_right() {
    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-exec".into(),
        "exec_command".into(),
        ToolStatus::Completed,
        r#"{"cmd":"echo hi"}"#.into(),
        Some(
            r#"{"state":"exited","exit_code":127,"output":"nope\n","wall_time_seconds":0.06}"#
                .into(),
        ),
        None,
    );
    tool.expanded = true;
    let lines = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 72);
    let title: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        title.contains("$ echo hi") || title.contains("echo hi"),
        "command on the left: {title}"
    );
    assert!(
        title.contains("exit 127"),
        "exit badge on the right: {title}"
    );
    assert!(
        title.contains("60ms"),
        "duration on the title right zone: {title}"
    );
    assert!(
        !title.contains('✓'),
        "must not show protocol success glyph over a failed command: {title}"
    );
    // Card bg follows command failure, not ToolStatus::Completed.
    assert_eq!(
        lines[1].spans[0].style.bg,
        Some(theme.tool_error_bg),
        "card should use error bg for nonzero exit"
    );
    let body: String = lines
        .iter()
        .skip(2)
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("nope"), "{body}");
    assert!(
        !body.contains("exit 127") && !body.contains("✗ 127"),
        "body must not repeat exit headline: {body}"
    );
}

#[test]
fn tool_block_renders_readable_fields_not_raw_json() {
    let theme = Theme::dark();
    let mut tool = ToolEntry::new(
        "c-read".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"Cargo.toml","offset":1}"#.into(),
        Some(r#"{"content":"[package]\nname = \"piko\"\n","totalLines":2,"linesRead":2}"#.into()),
        None,
    );

    let collapsed = component_lines(
        &TimelineComponent::Tool(tool.clone()),
        true,
        false,
        &theme,
        60,
    );
    let collapsed_text: String = collapsed
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        collapsed_text.contains("Cargo.toml"),
        "collapsed title should show path: {collapsed_text}"
    );
    assert!(
        !collapsed_text.contains(r#"{"path""#),
        "collapsed should not dump raw args JSON: {collapsed_text}"
    );

    tool.expanded = true;
    let expanded = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 60);
    let expanded_text: String = expanded
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        expanded_text.contains("Cargo.toml"),
        "expanded title should keep path: {expanded_text}"
    );
    assert!(
        expanded_text.contains("[package]"),
        "expanded body should show file content: {expanded_text}"
    );
}
