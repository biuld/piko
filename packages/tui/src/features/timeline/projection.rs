use super::*;

/// Split a committed protocol message into timeline components. Assistant
/// messages split provider-side ("upstream") tool activity/approval blocks
/// out of the text bubble into their own tool cards, mirroring a normal tool
/// call card.
pub(super) fn components_from_message(
    id: String,
    message: &piko_protocol::Message,
    _expanded: &HashMap<String, bool>,
    upstream_tools: &HashMap<String, ToolEntry>,
) -> Vec<TimelineComponent> {
    match message {
        piko_protocol::Message::User { timestamp, .. } => {
            vec![TimelineComponent::User(UserMessageComponent {
                id: ComponentId::MessageId(id),
                text: crate::text::message_to_text(message),
                timestamp: *timestamp,
            })]
        }
        piko_protocol::Message::Assistant {
            content,
            stop_reason,
            error_message,
            timestamp,
            ..
        } => {
            // Interleave the text/thinking/image runs with the upstream tool
            // cards at their content-block positions, mirroring a normal tool
            // call (text → card → text). The cards come from client-core's
            // first-class upstream `ToolItem`s.
            let mut out = Vec::new();
            let mut pending: Vec<piko_protocol::ContentBlock> = Vec::new();
            // One card per upstream activity/approval id: in_progress and
            // completed lifecycle blocks share the same id.
            let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
            let flush = |out: &mut Vec<TimelineComponent>,
                         pending: &mut Vec<piko_protocol::ContentBlock>| {
                if pending.is_empty() {
                    return;
                }
                out.push(TimelineComponent::Assistant(AssistantMessageComponent {
                    id: ComponentId::MessageId(id.clone()),
                    blocks: pending.iter().cloned().map(ContentBlock::from).collect(),
                    stop_reason: stop_reason.clone(),
                    error_message: error_message.clone(),
                    timestamp: *timestamp,
                }));
                pending.clear();
            };
            for block in content {
                match block {
                    piko_protocol::ContentBlock::UpstreamToolActivity { activity_id, .. } => {
                        flush(&mut out, &mut pending);
                        if emitted.insert(activity_id.as_str())
                            && let Some(tool) = upstream_tools.get(activity_id)
                        {
                            out.push(TimelineComponent::Tool(tool.clone()));
                        }
                    }
                    piko_protocol::ContentBlock::UpstreamToolApproval { approval_id, .. } => {
                        flush(&mut out, &mut pending);
                        if emitted.insert(approval_id.as_str())
                            && let Some(tool) = upstream_tools.get(approval_id)
                        {
                            out.push(TimelineComponent::Tool(tool.clone()));
                        }
                    }
                    other => pending.push(other.clone()),
                }
            }
            flush(&mut out, &mut pending);
            out
        }
        _ => Vec::new(),
    }
}

/// Project a client-core tool item into a TUI tool entry (shared by the
/// standalone `CoreItem::Tool` render and the upstream interleave path).
pub(super) fn project_tool_item(
    tool: &piko_client_core::timeline::ToolItem,
    expanded: &HashMap<String, bool>,
) -> ToolEntry {
    let result = if !tool.result_content.is_empty() {
        Some(protocol_blocks_to_text(&tool.result_content))
    } else {
        tool.result.as_ref().map(super::tool_format::json_for_entry)
    };
    let args = tool
        .partial_json
        .clone()
        .unwrap_or_else(|| super::tool_format::json_for_entry(&tool.args));
    let mut projected = ToolEntry::new(
        tool.tool_call_id.clone(),
        tool.tool_name.clone(),
        map_tool_status(tool.status),
        args,
        result,
        tool.parent_message_id.clone(),
    );
    projected.result_details = tool
        .result_details
        .as_ref()
        .map(super::tool_format::json_for_entry);
    projected.upstream = tool.upstream.as_ref().map(|upstream| {
        Box::new(UpstreamInfo {
            kind: upstream.kind.clone(),
            summary: upstream.summary.clone(),
            action: upstream.action.clone(),
        })
    });
    projected.expanded = expanded.get(&projected.id).copied().unwrap_or(false);
    projected
}

/// One upstream card anchored to a live draft split point.
#[derive(Clone)]
pub(super) struct DraftSlice {
    pub tool: ToolEntry,
    /// Char counts of the assistant text/thinking already emitted when the
    /// upstream tool started.
    pub text_before: usize,
    pub thinking_before: usize,
}

/// Split a live assistant draft into `text-before → card → … → text-after`
/// using the upstream anchor snapshots, mirroring a normal tool call.
pub(super) fn components_from_draft(
    draft: &piko_client_core::RealtimeDraft,
    slices: &[DraftSlice],
) -> Vec<TimelineComponent> {
    let blocks: Vec<(piko_client_core::RealtimeContentKind, String)> = draft
        .content_segments
        .iter()
        .filter(|segment| !segment.text.is_empty())
        .map(|segment| (segment.kind, segment.text.clone()))
        .collect();
    let mut out = Vec::new();
    let mut text_lo = 0usize;
    let mut thinking_lo = 0usize;
    for slice in slices {
        let before = clip_blocks(
            &blocks,
            text_lo,
            slice.text_before,
            thinking_lo,
            slice.thinking_before,
        );
        if !before.is_empty() {
            out.push(assistant_component(&draft.message_id, before));
        }
        out.push(TimelineComponent::Tool(slice.tool.clone()));
        text_lo = slice.text_before;
        thinking_lo = slice.thinking_before;
    }
    let tail = clip_blocks(
        &blocks,
        text_lo,
        draft.text().chars().count(),
        thinking_lo,
        draft.thinking().chars().count(),
    );
    if !tail.is_empty() {
        out.push(assistant_component(&draft.message_id, tail));
    }
    out
}

fn assistant_component(message_id: &str, blocks: Vec<ContentBlock>) -> TimelineComponent {
    TimelineComponent::Assistant(AssistantMessageComponent {
        id: ComponentId::MessageId(message_id.to_string()),
        blocks,
        stop_reason: None,
        error_message: None,
        timestamp: None,
    })
}

/// Clip the interleaved text/thinking blocks to `[text_lo..text_hi)` in the
/// text stream and `[think_lo..think_hi)` in the thinking stream, preserving
/// the original segment order.
fn clip_blocks(
    blocks: &[(piko_client_core::RealtimeContentKind, String)],
    text_lo: usize,
    text_hi: usize,
    think_lo: usize,
    think_hi: usize,
) -> Vec<ContentBlock> {
    let mut out = Vec::new();
    let mut text_pos = 0usize;
    let mut think_pos = 0usize;
    for (kind, text) in blocks {
        let len = text.chars().count();
        let (seg_start, seg_end, lo, hi) = match kind {
            piko_client_core::RealtimeContentKind::Text => {
                (text_pos, text_pos + len, text_lo, text_hi)
            }
            piko_client_core::RealtimeContentKind::Thinking => {
                (think_pos, think_pos + len, think_lo, think_hi)
            }
        };
        let take_start = seg_start.max(lo);
        let take_end = seg_end.min(hi);
        if take_end > take_start {
            let slice = char_slice(text, take_start - seg_start, take_end - seg_start);
            let block = match kind {
                piko_client_core::RealtimeContentKind::Text => ContentBlock::Text(slice),
                piko_client_core::RealtimeContentKind::Thinking => ContentBlock::Thinking(slice),
            };
            out.push(block);
        }
        if matches!(kind, piko_client_core::RealtimeContentKind::Text) {
            text_pos = seg_end;
        } else {
            think_pos = seg_end;
        }
    }
    out
}

fn char_slice(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

pub(super) fn component_from_session_entry(
    entry: &piko_protocol::SessionTreeEntry,
) -> Option<TimelineComponent> {
    use piko_protocol::SessionTreeEntry;
    match entry {
        SessionTreeEntry::ModelChange(change) => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "model",
                text: format!("changed to {}/{}", change.provider, change.model_id),
            }))
        }
        SessionTreeEntry::ThinkingLevelChange(change) => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "thinking",
                text: format!("changed to {}", change.thinking_level),
            }))
        }
        SessionTreeEntry::ActiveToolsChange(change) if !change.active_tool_names.is_empty() => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "tools",
                text: change.active_tool_names.join(", "),
            }))
        }
        SessionTreeEntry::Compaction(compaction) => {
            Some(TimelineComponent::Summary(SummaryComponent {
                id: ComponentId::EntryId(compaction.id.clone()),
                kind: SummaryKind::Compaction,
                text: compaction.summary.clone(),
            }))
        }
        SessionTreeEntry::BranchSummary(summary) => {
            Some(TimelineComponent::Summary(SummaryComponent {
                id: ComponentId::EntryId(summary.id.clone()),
                kind: SummaryKind::Branch,
                text: summary.summary.clone(),
            }))
        }
        SessionTreeEntry::CustomMessage(custom) if custom.display => {
            Some(TimelineComponent::CustomMessage(CustomMessageComponent {
                id: ComponentId::EntryId(custom.id.clone()),
                custom_type: custom.custom_type.clone(),
                content: custom.content.clone(),
            }))
        }
        _ => None,
    }
}

pub(super) fn protocol_blocks_to_text(blocks: &[piko_protocol::ContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            piko_protocol::ContentBlock::Text { text } => text.clone(),
            piko_protocol::ContentBlock::Thinking { thinking, .. } => thinking.clone(),
            piko_protocol::ContentBlock::Image { mime_type, .. } => format!("[image: {mime_type}]"),
            other => other.text_projection(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn map_tool_status(status: piko_client_core::ToolStatus) -> crate::app::ToolStatus {
    match status {
        piko_client_core::ToolStatus::Running => crate::app::ToolStatus::Running,
        piko_client_core::ToolStatus::Completed => crate::app::ToolStatus::Completed,
        piko_client_core::ToolStatus::Failed => crate::app::ToolStatus::Failed,
        piko_client_core::ToolStatus::Cancelled => crate::app::ToolStatus::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_bubble_strips_upstream_blocks() {
        let message = piko_protocol::Message::Assistant {
            content: vec![
                piko_protocol::ContentBlock::Text {
                    text: "checking".into(),
                },
                piko_protocol::ContentBlock::UpstreamToolActivity {
                    activity_id: "act-1".into(),
                    tool_name: "web_search".into(),
                    kind: "search".into(),
                    status: piko_protocol::messages::UpstreamActivityStatus::InProgress,
                    arguments: Some(serde_json::json!({ "type": "search", "query": "深圳天气" })),
                    action: Some(piko_protocol::messages::UpstreamAction::Search {
                        queries: vec!["深圳天气".into()],
                    }),
                },
                piko_protocol::ContentBlock::UpstreamToolApproval {
                    approval_id: "appr-1".into(),
                    tool_name: "web_search".into(),
                    summary: "needs consent".into(),
                },
            ],
            checkpoint: None,
            provider: "test".into(),
            model: "test".into(),
            usage: None,
            stop_reason: Some("stop".into()),
            error_message: None,
            timestamp: None,
        };

        let components =
            components_from_message("a-1".into(), &message, &HashMap::new(), &HashMap::new());

        assert_eq!(components.len(), 1, "no tool cards from TUI");
        let TimelineComponent::Assistant(assistant) = &components[0] else {
            panic!("expected an assistant bubble");
        };
        assert_eq!(assistant.blocks.len(), 1);
        assert!(matches!(&assistant.blocks[0], ContentBlock::Text(t) if t == "checking"));
    }

    #[test]
    fn upstream_cards_interleave_with_text_runs() {
        let message = piko_protocol::Message::Assistant {
            content: vec![
                piko_protocol::ContentBlock::Text {
                    text: "before".into(),
                },
                piko_protocol::ContentBlock::UpstreamToolActivity {
                    activity_id: "ws_1".into(),
                    tool_name: "web_search".into(),
                    kind: "search".into(),
                    status: piko_protocol::messages::UpstreamActivityStatus::Completed,
                    arguments: Some(serde_json::json!({ "query": "深圳天气" })),
                    action: Some(piko_protocol::messages::UpstreamAction::Search {
                        queries: vec!["深圳天气".into()],
                    }),
                },
                piko_protocol::ContentBlock::Text {
                    text: "after".into(),
                },
            ],
            checkpoint: None,
            provider: "test".into(),
            model: "test".into(),
            usage: None,
            stop_reason: Some("stop".into()),
            error_message: None,
            timestamp: None,
        };
        let mut card = ToolEntry::new(
            "ws_1".into(),
            "web_search".into(),
            crate::app::ToolStatus::Completed,
            r#"{"query":"深圳天气"}"#.into(),
            None,
            None,
        );
        card.upstream = Some(Box::new(UpstreamInfo {
            kind: "search".into(),
            summary: None,
            action: None,
        }));
        let mut upstream_tools = HashMap::new();
        upstream_tools.insert("ws_1".into(), card);

        let components =
            components_from_message("a-1".into(), &message, &HashMap::new(), &upstream_tools);
        assert_eq!(components.len(), 3, "text → card → text");
        assert!(matches!(components[0], TimelineComponent::Assistant(_)));
        assert!(matches!(components[1], TimelineComponent::Tool(_)));
        assert!(matches!(components[2], TimelineComponent::Assistant(_)));
        if let TimelineComponent::Assistant(trailing) = &components[2] {
            assert!(matches!(&trailing.blocks[0], ContentBlock::Text(t) if t == "after"));
        }
    }

    #[test]
    fn live_draft_splits_around_upstream_card() {
        use piko_client_core::RealtimeContentKind;
        use piko_client_core::RealtimeContentSegment;

        let draft = piko_client_core::RealtimeDraft {
            message_id: "msg-1".into(),
            last_delta_seq: 3,
            content_segments: vec![
                RealtimeContentSegment {
                    kind: RealtimeContentKind::Thinking,
                    content_index: 0,
                    text: "think".into(),
                },
                RealtimeContentSegment {
                    kind: RealtimeContentKind::Text,
                    content_index: 0,
                    text: "beforeafter".into(),
                },
            ],
            live_order: 0,
        };
        let card = ToolEntry::new(
            "ws_1".into(),
            "web_search".into(),
            crate::app::ToolStatus::Completed,
            r#"{"query":"深圳天气"}"#.into(),
            None,
            Some("msg-1".into()),
        );
        let slice = DraftSlice {
            tool: card,
            text_before: "before".chars().count(),
            thinking_before: "think".chars().count(),
        };
        let components = components_from_draft(&draft, &[slice]);
        assert_eq!(components.len(), 3, "text → card → text");
        // Before bubble keeps thinking then the text prefix.
        if let TimelineComponent::Assistant(before) = &components[0] {
            assert_eq!(before.blocks.len(), 2);
            assert!(matches!(&before.blocks[0], ContentBlock::Thinking(t) if t == "think"));
            assert!(matches!(&before.blocks[1], ContentBlock::Text(t) if t == "before"));
        } else {
            panic!("expected before bubble");
        }
        assert!(matches!(&components[1], TimelineComponent::Tool(t) if t.id == "ws_1"));
        if let TimelineComponent::Assistant(after) = &components[2] {
            assert!(matches!(&after.blocks[0], ContentBlock::Text(t) if t == "after"));
        } else {
            panic!("expected after bubble");
        }
    }
}
