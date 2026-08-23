use super::component_lines;
use crate::app::ToolStatus;
use crate::features::timeline::{Timeline, TimelineComponent, ToolEntry, UpstreamInfo};
use crate::theme::Theme;
use piko_protocol::StreamItemPatch;
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::messages::UpstreamAction;

fn upstream_tool(
    id: &str,
    kind: &str,
    summary: Option<&str>,
    args: &str,
    status: ToolStatus,
    expanded: bool,
    action: Option<UpstreamAction>,
) -> ToolEntry {
    let mut tool = ToolEntry::new(
        id.into(),
        "web_search".into(),
        status,
        args.into(),
        None,
        None,
    );
    tool.upstream = Some(Box::new(UpstreamInfo {
        kind: kind.into(),
        summary: summary.map(str::to_string),
        action,
    }));
    tool.expanded = expanded;
    tool
}

#[test]
fn upstream_activity_card_renders_like_regular_tool() {
    let theme = Theme::dark();
    let lines = component_lines(
        &TimelineComponent::Tool(upstream_tool(
            "act-1",
            "search",
            None,
            r#"{"queries":["深圳天气","ws_call_id=call_00_x"]}"#,
            ToolStatus::Running,
            false,
            Some(UpstreamAction::Search {
                queries: vec!["深圳天气".to_string()],
            }),
        )),
        true,
        false,
        &theme,
        40,
    );
    let plain = render_plain(&lines);
    assert!(plain.contains('▸'), "disclosure marker missing: {plain}");
    assert!(plain.contains("web_search"), "tool name missing: {plain}");
    assert!(
        plain.contains("深圳天气"),
        "query should show on title: {plain}"
    );
    assert!(plain.contains('○'), "running glyph missing: {plain}");
    assert!(
        !plain.contains("[upstream tool:"),
        "raw text leaked: {plain}"
    );
}

#[test]
fn upstream_approval_card_exposes_summary_on_expand() {
    let theme = Theme::dark();
    let lines = component_lines(
        &TimelineComponent::Tool(upstream_tool(
            "appr-1",
            "",
            Some("search requires consent"),
            "",
            ToolStatus::Running,
            true,
            None,
        )),
        true,
        false,
        &theme,
        40,
    );
    let plain = render_plain(&lines);
    assert!(
        plain.contains("approval"),
        "approval label missing: {plain}"
    );
    assert!(
        plain.contains("search requires consent"),
        "summary missing: {plain}"
    );
}

#[test]
fn open_page_card_shows_action_type_not_catalog_kind() {
    let theme = Theme::dark();
    let lines = component_lines(
        &TimelineComponent::Tool(upstream_tool(
            "act-2",
            "search",
            None,
            // Raw args echo; the typed action drives display.
            r#"{"type":"open_page","url":"https://example.test/page.html"}"#,
            ToolStatus::Completed,
            true,
            Some(UpstreamAction::OpenPage {
                url: "https://example.test/page.html".to_string(),
            }),
        )),
        true,
        false,
        &theme,
        80,
    );
    let plain = render_plain(&lines);
    assert!(plain.contains("url"), "url meta missing: {plain}");
    assert!(
        plain.contains("open_page"),
        "action type should be open_page: {plain}"
    );
    assert!(
        !plain.contains("type search"),
        "catalog kind leaked as type: {plain}"
    );
}

#[test]
fn live_upstream_card_stays_visible_after_complete() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    // message started + before text
    apply_patch(
        &mut timeline,
        "msg-1",
        0,
        &RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    );
    apply_patch(
        &mut timeline,
        "msg-1",
        1,
        &RealtimeDelta::Text {
            content_index: 0,
            delta: "checking rate".into(),
        },
    );
    // web search running
    apply_patch(
        &mut timeline,
        "msg-1",
        2,
        &RealtimeDelta::UpstreamActivity {
            activity_id: "ws_1".into(),
            tool_name: "web_search".into(),
            kind: "search".into(),
            status: piko_protocol::messages::UpstreamActivityStatus::InProgress,
            arguments: None,
            action: None,
        },
    );
    let before_complete = render_plain(&timeline.render_lines(&theme, 60));
    assert!(
        before_complete.contains("web_search"),
        "running card missing"
    );

    // web search completed
    apply_patch(
        &mut timeline,
        "msg-1",
        3,
        &RealtimeDelta::UpstreamActivity {
            activity_id: "ws_1".into(),
            tool_name: "web_search".into(),
            kind: "search".into(),
            status: piko_protocol::messages::UpstreamActivityStatus::Completed,
            arguments: Some(serde_json::json!({
                "type": "search",
                "queries": ["USD CNY", "ws_call_id=call_00_x"],
            })),
            action: Some(UpstreamAction::Search {
                queries: vec!["USD CNY".to_string()],
            }),
        },
    );
    let after_complete = render_plain(&timeline.render_lines(&theme, 60));
    assert!(
        after_complete.contains("web_search"),
        "card vanished after complete"
    );
    assert!(
        after_complete.contains("USD CNY"),
        "query missing: {after_complete}"
    );

    // after text streams
    apply_patch(
        &mut timeline,
        "msg-1",
        4,
        &RealtimeDelta::Text {
            content_index: 0,
            delta: " the rate is 6.72".into(),
        },
    );
    let final_text = render_plain(&timeline.render_lines(&theme, 60));
    assert!(
        final_text.contains("the rate is 6.72"),
        "after text missing"
    );
    assert!(final_text.contains("web_search"), "card missing at end");
}

fn render_plain(lines: &[ratatui::text::Line<'static>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_patch(timeline: &mut Timeline, msg: &str, seq: u64, delta: &RealtimeDelta) {
    let patch = StreamItemPatch::from_realtime_delta(
        Some("s".into()),
        Some("root".into()),
        msg,
        Some(seq),
        delta,
    )
    .pop()
    .expect("patch");
    timeline.apply_stream_item(&patch);
}
