#![allow(unused_imports)]

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
