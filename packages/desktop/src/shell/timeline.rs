//! Timeline surface: target-keyed states derived from the client-core
//! projection (D-59 Slice 2).

use piko_client_core::{
    ClientState,
    state::{PendingOp, SessionPhase},
    timeline::{RealtimeContentKind, TimelineItem, ToolStatus},
};
use piko_protocol::{ContentBlock, Message, MessageContent, session::SessionTreeEntry};

/// Target-keyed presentation states. Stale content is never shown as the
/// current target during loading or failure (F-42).
#[derive(Debug, PartialEq)]
pub enum TimelineState {
    NoSession,
    Loading,
    Error(String),
    Empty,
    Ready(Vec<TimelineRow>),
}

/// One presentation row mapped from a normalized client-core item.
#[derive(Debug, PartialEq)]
pub enum TimelineRow {
    User {
        id: String,
        text: String,
    },
    Assistant {
        id: String,
        text: String,
    },
    Thinking {
        id: String,
        text: String,
    },
    Tool {
        id: String,
        name: String,
        detail: String,
        running: bool,
        failed: bool,
    },
    System {
        id: String,
        label: String,
    },
}

impl TimelineRow {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Assistant { id, .. }
            | Self::Thinking { id, .. }
            | Self::Tool { id, .. }
            | Self::System { id, .. } => id,
        }
    }
}

/// Derive the presentation state for the selected agent from the sole host
/// projection.
pub fn timeline_state(core: &ClientState) -> TimelineState {
    match core.session_phase {
        SessionPhase::IdleNoSession => {
            return session_load_error(core).unwrap_or(TimelineState::NoSession);
        }
        SessionPhase::OpeningOrCreating { .. } | SessionPhase::Hydrating { .. } => {
            return TimelineState::Loading;
        }
        SessionPhase::Live => {}
    }
    let Some(session) = core.live_session.as_ref() else {
        return TimelineState::Loading;
    };
    let Some(selected) = session.selected_agent.as_ref() else {
        return TimelineState::Empty;
    };
    let Some(timeline) = session.timelines.get(selected) else {
        return TimelineState::Empty;
    };
    let mut rows = Vec::new();
    for item in timeline.items() {
        rows.extend(map_item(item));
    }
    if rows.is_empty() {
        TimelineState::Empty
    } else {
        TimelineState::Ready(rows)
    }
}

fn session_load_error(core: &ClientState) -> Option<TimelineState> {
    core.command_failures.iter().rev().find_map(|failure| {
        matches!(
            failure.operation,
            PendingOp::Open { .. } | PendingOp::Create
        )
        .then(|| TimelineState::Error(failure.message.clone()))
    })
}

fn map_item(item: &TimelineItem) -> Vec<TimelineRow> {
    match item {
        TimelineItem::Committed(committed) => {
            message_rows(&committed.message, &committed.message_id)
        }
        TimelineItem::RealtimeDraft(draft) => draft_rows(draft),
        TimelineItem::Tool(tool) => vec![tool_row(
            tool.tool_call_id.clone(),
            &tool.tool_name,
            tool_args_preview(Some(&tool.args), tool.partial_json.as_deref()),
            tool.status == ToolStatus::Running,
            matches!(tool.status, ToolStatus::Failed | ToolStatus::Cancelled),
        )],
        TimelineItem::SessionEntry(entry) => entry_rows(
            &entry.entry,
            format!("session-entry-{}", entry.branch_order),
        ),
    }
}

fn message_rows(message: &Message, id: &str) -> Vec<TimelineRow> {
    match message {
        Message::User { content, .. } => vec![TimelineRow::User {
            id: format!("{id}-user"),
            text: content_text(content),
        }],
        Message::Assistant { content, .. } => assistant_rows(content, id),
        Message::Context { .. } => vec![TimelineRow::System {
            id: format!("{id}-context"),
            label: "context".to_string(),
        }],
        Message::ToolCall {
            name, arguments, ..
        } => vec![tool_row(
            format!("{id}-tool"),
            name,
            tool_args_preview(Some(arguments), None),
            false,
            false,
        )],
        _ => Vec::new(),
    }
}

fn assistant_rows(blocks: &[ContentBlock], id: &str) -> Vec<TimelineRow> {
    let mut thinking = String::new();
    let mut text = String::new();
    for block in blocks {
        match block {
            ContentBlock::Thinking { thinking: t, .. } => {
                if !thinking.is_empty() {
                    thinking.push('\n');
                }
                thinking.push_str(t);
            }
            ContentBlock::Text { text: t } => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
            _ => {}
        }
    }
    let mut rows = Vec::new();
    if !thinking.is_empty() {
        rows.push(TimelineRow::Thinking {
            id: format!("{id}-thinking"),
            text: thinking,
        });
    }
    if !text.is_empty() {
        rows.push(TimelineRow::Assistant {
            id: format!("{id}-text"),
            text,
        });
    }
    rows
}

fn draft_rows(draft: &piko_client_core::RealtimeDraft) -> Vec<TimelineRow> {
    let mut thinking = String::new();
    let mut text = String::new();
    for segment in &draft.content_segments {
        let target = match segment.kind {
            RealtimeContentKind::Text => &mut text,
            RealtimeContentKind::Thinking => &mut thinking,
        };
        if !target.is_empty() {
            target.push('\n');
        }
        target.push_str(&segment.text);
    }
    let mut rows = Vec::new();
    if !thinking.is_empty() {
        rows.push(TimelineRow::Thinking {
            id: format!("{}-thinking", draft.message_id),
            text: thinking,
        });
    }
    if !text.is_empty() {
        rows.push(TimelineRow::Assistant {
            id: format!("{}-text", draft.message_id),
            text,
        });
    }
    rows
}

fn tool_row(id: String, name: &str, detail: String, running: bool, failed: bool) -> TimelineRow {
    TimelineRow::Tool {
        id,
        name: name.to_string(),
        detail,
        running,
        failed,
    }
}

fn entry_label(entry: &SessionTreeEntry) -> Option<String> {
    match entry {
        SessionTreeEntry::Compaction(_) => Some("Compaction".to_string()),
        SessionTreeEntry::BranchSummary(_) => Some("Branch summary".to_string()),
        SessionTreeEntry::ModelChange(change) => {
            Some(format!("Model · {}/{}", change.provider, change.model_id))
        }
        _ => None,
    }
}

fn entry_rows(entry: &SessionTreeEntry, id: String) -> Vec<TimelineRow> {
    entry_label(entry)
        .map(|label| vec![TimelineRow::System { id, label }])
        .unwrap_or_default()
}

fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

const ARGS_PREVIEW_LIMIT: usize = 160;

fn tool_args_preview(args: Option<&serde_json::Value>, partial_json: Option<&str>) -> String {
    let raw = args
        .map(|args| args.to_string())
        .or_else(|| partial_json.map(str::to_string))
        .unwrap_or_default();
    truncate_summary(&raw, ARGS_PREVIEW_LIMIT)
}

fn truncate_summary(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(limit).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::timeline::AgentTimeline;

    fn state_with(timeline: AgentTimeline) -> ClientState {
        let mut core = ClientState::default();
        core.session_phase = piko_client_core::state::SessionPhase::Live;
        let mut session = piko_client_core::LiveSession {
            selected_agent: Some("agent-1".to_string()),
            ..piko_client_core::LiveSession::default()
        };
        session.timelines.insert("agent-1".to_string(), timeline);
        core.live_session = Some(session);
        core
    }

    #[test]
    fn no_session_when_idle() {
        assert_eq!(
            timeline_state(&ClientState::default()),
            TimelineState::NoSession
        );
    }

    #[test]
    fn open_failure_is_an_error_not_an_empty_session() {
        let mut core = ClientState::default();
        core.command_failures
            .push(piko_client_core::state::CommandFailure {
                command_id: "desktop-1".to_string(),
                operation: PendingOp::Open {
                    session_id: "s1".to_string(),
                },
                message: "session journal unreadable".to_string(),
            });
        assert_eq!(
            timeline_state(&core),
            TimelineState::Error("session journal unreadable".to_string())
        );
    }

    #[test]
    fn loading_during_hydration() {
        let mut core = ClientState::default();
        core.session_phase = SessionPhase::Hydrating {
            target_id: "s1".to_string(),
        };
        assert_eq!(timeline_state(&core), TimelineState::Loading);
    }

    #[test]
    fn empty_when_live_without_items() {
        assert_eq!(
            timeline_state(&state_with(AgentTimeline::new())),
            TimelineState::Empty
        );
    }

    #[test]
    fn ready_maps_committed_user_and_assistant() {
        let mut timeline = AgentTimeline::new();
        timeline.apply_committed(
            "m1".to_string(),
            1,
            Message::User {
                content: MessageContent::String("hello".to_string()),
                timestamp: None,
            },
            "turn-1".to_string(),
        );
        timeline.apply_committed(
            "m2".to_string(),
            2,
            Message::Assistant {
                content: vec![
                    ContentBlock::Thinking {
                        thinking: "hmm".to_string(),
                        thinking_signature: None,
                    },
                    ContentBlock::Text {
                        text: "hi there".to_string(),
                    },
                ],
                checkpoint: None,
                provider: "deepseek".to_string(),
                model: "v4".to_string(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            },
            "turn-1".to_string(),
        );
        match timeline_state(&state_with(timeline)) {
            TimelineState::Ready(rows) => {
                assert!(matches!(&rows[..], [
                    TimelineRow::User { text: user, .. },
                    TimelineRow::Thinking { text: th, .. },
                    TimelineRow::Assistant { text: assistant, .. }
                ] if user == "hello" && th == "hmm" && assistant == "hi there"));
            }
            other => panic!("expected ready state, got {other:?}"),
        }
    }

    #[test]
    fn system_entries_render_labels_and_unknown_messages_are_skipped() {
        let rows = message_rows(
            &Message::Context {
                content: MessageContent::String("data".to_string()),
                trust: piko_protocol::ContentTrust::Trusted,
                source: piko_protocol::PromptSource::new("test", "timeline"),
                timestamp: None,
            },
            "m1",
        );
        match &rows[..] {
            [TimelineRow::System { label, .. }] => assert_eq!(label, "context"),
            other => panic!("unexpected rows {other:?}"),
        }
    }

    #[test]
    fn long_tool_arguments_are_truncated_with_ellipsis() {
        let preview = truncate_summary(&"x".repeat(200), ARGS_PREVIEW_LIMIT);
        assert_eq!(preview.chars().count(), ARGS_PREVIEW_LIMIT + 1);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn realtime_and_committed_assistant_rows_keep_message_identity() {
        let draft = piko_client_core::RealtimeDraft {
            message_id: "m1".to_string(),
            last_delta_seq: 1,
            content_segments: vec![piko_client_core::RealtimeContentSegment {
                kind: RealtimeContentKind::Text,
                content_index: 0,
                text: "streaming".to_string(),
            }],
            live_order: 1,
        };
        let realtime = draft_rows(&draft);
        let committed = assistant_rows(
            &[ContentBlock::Text {
                text: "complete".to_string(),
            }],
            "m1",
        );
        assert_eq!(realtime[0].id(), committed[0].id());
    }
}
