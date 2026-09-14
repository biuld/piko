use piko_protocol::{
    HistoryAvailability, HistoryItemContent, HistoryItemDetail, HistoryItemKind, HistoryItemRef,
    HistoryProvenance, HistoryRelation, HistoryStreamItem, Message, MessageContent,
};
use ratatui::text::Line;

use super::{row_line, tab_lines};
use crate::features::history::{DetailTab, HistoryRow};
use crate::theme::Theme;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_string()
}

fn stream_item(kind: &str, summary: &str) -> HistoryStreamItem {
    let badge = match kind {
        "model_step" => "STEP 1",
        "message" => "ASSISTANT",
        _ => "USER",
    };
    HistoryStreamItem {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        revision: 2,
        event_index: 0,
        committed_at: 2,
        kind: HistoryItemKind::new(kind),
        badge: badge.into(),
        relation: HistoryRelation::default(),
        summary: summary.into(),
        status: None,
        duration_ms: None,
        has_detail: true,
    }
}

#[test]
fn stream_rows_use_kind_labels_instead_of_debug_enums() {
    let theme = Theme::dark();
    let item = stream_item("model_step", "model step 1 committed: Completed");
    let line = row_line(80, false, &HistoryRow::Stream(item), &theme);
    let shown = text(&line);
    assert!(shown.contains("Step"));
    assert!(shown.contains("ended"));
    assert!(shown.contains("model step 1 committed"));
    assert!(!shown.contains("event:2:0"));
}

#[test]
fn tool_call_tabs_reuse_the_timeline_tool_card_presenter() {
    let theme = Theme::dark();
    let detail = HistoryItemDetail {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::Message {
            message_id: "tool-message-1".into(),
            message: Message::ToolCall {
                id: "call-1".into(),
                name: "exec_command".into(),
                arguments: serde_json::json!({"cmd": "cargo test", "workdir": "/project"}),
                model: None,
                provider: None,
                timestamp: None,
            },
        }),
        diagnostic: Some(Box::new(piko_protocol::TrajectoryRecord::ToolCall(
            piko_protocol::TrajectoryToolCallRecord {
                identity: piko_protocol::TrajectoryIdentity {
                    session_id: "s".into(),
                    agent_instance_id: "agent_s_root".into(),
                    root_input_id: "input-1".into(),
                },
                call_id: "call-1".into(),
                tool_name: "exec_command".into(),
                arguments: Some(serde_json::json!({"cmd": "cargo test"})),
                status: piko_protocol::TrajectoryToolCallStatus::Completed,
                started_at: 1,
                finished_at: Some(121),
                duration_ms: Some(120),
                result: Some(serde_json::json!({
                    "state": "exited",
                    "exit_code": 0,
                    "output": "tests passed",
                    "wall_time_seconds": 0.12
                })),
                error: None,
                message_id: None,
            },
        ))),
    };

    let payload = tab_lines(DetailTab::Payload, Some(&detail), None, &theme, 72)
        .iter()
        .map(text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(payload.contains("exec_command"));
    assert!(payload.contains("$ cargo test"));
    assert!(payload.contains("cwd"));
    assert!(!payload.contains("\"cmd\""));

    let result = tab_lines(DetailTab::Result, Some(&detail), None, &theme, 72)
        .iter()
        .map(text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(result.contains("exit 0"));
    assert!(result.contains("tests passed"));
    assert!(!result.contains("\"exit_code\""));
}

#[test]
fn message_rows_show_role_badges_and_duration() {
    let theme = Theme::dark();
    let mut item = stream_item("message", "assistant message committed");
    item.duration_ms = Some(21);
    let line = row_line(80, false, &HistoryRow::Stream(item), &theme);
    let shown = text(&line);
    assert!(shown.contains("ASSISTANT"));
    assert!(shown.contains("21ms"));
}

#[test]
fn payload_tab_shows_typed_message_content_not_json() {
    let theme = Theme::dark();
    let detail = HistoryItemDetail {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::Message {
            message_id: "m1".into(),
            message: Message::User {
                content: MessageContent::String("hello there".into()),
                timestamp: None,
            },
        }),
        diagnostic: None,
    };
    let lines = tab_lines(DetailTab::Payload, Some(&detail), None, &theme, 60);
    let shown = lines.iter().map(text).collect::<Vec<_>>().join("\n");
    assert!(shown.contains("user"));
    assert!(shown.contains("hello there"));
    assert!(!shown.contains("\"role\""));
}

#[test]
fn timing_tab_reports_absent_diagnostics() {
    let theme = Theme::dark();
    let detail = HistoryItemDetail {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::Input {
            input: piko_protocol::AgentInput {
                input_id: "input-1".into(),
                request_id: "request-1".into(),
                session_id: "s".into(),
                agent_instance_id: "agent_s_root".into(),
                origin: piko_protocol::AgentInputOrigin::User,
                delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
                content: MessageContent::String("explain".into()),
                submitted_at: 1,
                caller_agent_instance_id: None,
                detached_recipient_agent_instance_id: None,
            },
        }),
        diagnostic: None,
    };
    let lines = tab_lines(DetailTab::Timing, Some(&detail), None, &theme, 60);
    let shown = lines.iter().map(text).collect::<Vec<_>>().join("\n");
    assert!(shown.contains("diagnostic timing was not recorded"));
    assert!(!shown.contains("0 ms"));
}

#[test]
fn result_tab_prefers_the_tool_result_message() {
    let theme = Theme::dark();
    let detail = HistoryItemDetail {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::Message {
            message_id: "m1".into(),
            message: Message::ToolResult {
                tool_call_id: "call-1".into(),
                tool_name: Some("bash".into()),
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "total 42".into(),
                }],
                details: None,
                is_error: None,
                timestamp: None,
            },
        }),
        diagnostic: None,
    };
    let lines = tab_lines(DetailTab::Result, Some(&detail), None, &theme, 60);
    let shown = lines.iter().map(text).collect::<Vec<_>>().join("\n");
    assert!(shown.contains("total 42"));
}

#[test]
fn mixed_blocks_preserve_thinking_text_and_non_text_content() {
    use piko_protocol::ContentBlock;
    let blocks = vec![
        ContentBlock::Thinking {
            thinking: "Reasoning evidence".into(),
            thinking_signature: None,
            duration_ms: None,
        },
        ContentBlock::Text {
            text: "Answer evidence".into(),
        },
        ContentBlock::Image {
            mime_type: "image/png".into(),
            data: "image payload".into(),
        },
    ];
    let shown = super::content::block_lines(&blocks, &Theme::dark(), 60)
        .iter()
        .map(text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(shown.contains("Thinking\nReasoning evidence"));
    assert!(shown.contains("Text\nAnswer evidence"));
    assert!(shown.contains("image/png"));
    assert!(!shown.contains("image payload"));
}
