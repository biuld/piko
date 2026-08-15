#![allow(unused_imports)]

use super::component_lines;
use crate::app::ToolStatus;
use crate::features::timeline::{
    AssistantMessageComponent, ComponentId, ContentBlock, Timeline, TimelineComponent,
    TimelineEntry, ToolEntry,
};
use crate::theme::Theme;
use piko_protocol::agent_runtime::RealtimeDelta;

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
