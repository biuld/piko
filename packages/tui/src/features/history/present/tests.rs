use piko_protocol::{
    AgentInputOrigin, AgentWorkProcessingStatus, HistoryAvailability, HistoryItemContent,
    HistoryItemDetail, HistoryItemKind, HistoryItemRef, HistoryItemSummary, HistoryProvenance,
    HistoryRelation, HistoryWorkSummary, Message, MessageContent,
};
use ratatui::text::Line;

use super::{detail_lines, row_line};
use crate::features::history::HistoryRow;
use crate::theme::Theme;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_string()
}

fn work() -> HistoryWorkSummary {
    HistoryWorkSummary {
        root_input_id: "input-aaaaaaaaaaaa".into(),
        agent_instance_id: "agent-1".into(),
        origin: AgentInputOrigin::User,
        input_preview: "plan the feature".into(),
        started_at: Some(1),
        finished_at: Some(2),
        outcome: Some(AgentWorkProcessingStatus::Succeeded),
        step_count: 3,
        tool_count: 2,
        message_count: 4,
        usage: None,
    }
}

#[test]
fn work_row_is_a_scan_line_not_an_id_dump() {
    let theme = Theme::dark();
    let line = row_line(120, false, &HistoryRow::Work(work()), &theme);
    let shown = text(&line);
    assert!(shown.contains("succeeded"));
    assert!(shown.contains("user"));
    assert!(shown.contains("plan the feature"));
    assert!(shown.contains("3 steps"));
    assert!(!shown.contains("input-aaaaaaaa"));
    assert!(!shown.contains("Succeeded"));
}

#[test]
fn item_row_uses_kind_labels_instead_of_debug_enums() {
    let theme = Theme::dark();
    let item = HistoryItemSummary {
        item_ref: HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        revision: 2,
        event_index: 0,
        committed_at: 2,
        kind: HistoryItemKind::new("model_step"),
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        relation: HistoryRelation::default(),
        summary: "assistant replied".into(),
        has_detail: true,
        children: Vec::new(),
    };
    let line = row_line(80, false, &HistoryRow::Item { item, depth: 0 }, &theme);
    let shown = text(&line);
    assert!(shown.contains("Step"));
    assert!(shown.contains("assistant replied"));
    assert!(!shown.contains("Fact"));
    assert!(!shown.contains("r2. 0"));
}

#[test]
fn message_detail_is_typed_not_json() {
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
    };
    let lines = detail_lines(&detail, &theme, 60);
    let shown = lines.iter().map(text).collect::<Vec<_>>().join("\n");
    assert!(shown.contains("Message · fact"));
    assert!(shown.contains("user"));
    assert!(shown.contains("hello there"));
    assert!(!shown.contains("\"role\""));
}

#[test]
fn complete_prompt_assembly_is_inspectable_beyond_eight_blocks() {
    use piko_protocol::*;
    let blocks = (0..12)
        .map(|i| PromptBlock {
            id: format!("block-{i}"),
            kind: PromptBlockKind::Context,
            authority: InstructionAuthority::None,
            trust: ContentTrust::Untrusted,
            source: PromptSource::new("fixture", format!("source-{i}")),
            content: format!("Complete content of block {i}"),
            content_digest: format!("digest-{i}"),
            cache_scope: CacheScope::NoCache,
        })
        .collect();
    let detail = HistoryItemDetail {
        item_ref: HistoryItemRef {
            revision: 12,
            token: "opaque".into(),
        },
        provenance: HistoryProvenance::Diagnostic,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::PromptAssembly {
            assembly: Box::new(TrajectoryAssemblyRecord {
                identity: TrajectoryIdentity {
                    session_id: "s".into(),
                    agent_instance_id: "a".into(),
                    root_input_id: "i".into(),
                },
                assembly_version: 1,
                prompt_digest: "complete-digest".into(),
                prompt: SemanticRunPrompt {
                    blocks,
                    ..Default::default()
                },
                tool_catalog: ResolvedToolCatalog::default(),
                recorded_at: 0,
            }),
        }),
    };
    let shown = detail_lines(&detail, &Theme::dark(), 48)
        .iter()
        .map(text)
        .collect::<Vec<_>>()
        .join("\n");
    for i in 0..12 {
        assert!(shown.contains(&format!("Complete content of block {i}")));
    }
    assert!(shown.contains("diagnostic"));
    assert!(shown.contains("complete-digest"));
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
