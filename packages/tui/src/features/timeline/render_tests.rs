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
    })
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
    let title = &collapsed[1].spans[0];
    assert!(title.content.contains('▸'));
    assert_eq!(title.style.fg, Some(theme.accent));
    assert_eq!(title.style.bg, Some(theme.tool_success_bg));

    tool.expanded = true;
    let expanded = component_lines(&TimelineComponent::Tool(tool), true, false, &theme, 40);
    assert!(expanded[1].spans[0].content.contains('▾'));
    assert!(expanded.len() > collapsed.len());
}
