use std::time::Instant;

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
            let mut thinking_index = 0u32;
            // One card per upstream activity/approval id: in_progress and
            // completed lifecycle blocks share the same id.
            let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for block in content {
                match block {
                    piko_protocol::ContentBlock::UpstreamToolActivity { activity_id, .. } => {
                        flush_committed_blocks(
                            &mut out,
                            &mut pending,
                            &id,
                            stop_reason,
                            error_message,
                            *timestamp,
                            &mut thinking_index,
                        );
                        if emitted.insert(activity_id.as_str())
                            && let Some(tool) = upstream_tools.get(activity_id)
                        {
                            out.push(TimelineComponent::Tool(tool.clone()));
                        }
                    }
                    piko_protocol::ContentBlock::UpstreamToolApproval { approval_id, .. } => {
                        flush_committed_blocks(
                            &mut out,
                            &mut pending,
                            &id,
                            stop_reason,
                            error_message,
                            *timestamp,
                            &mut thinking_index,
                        );
                        if emitted.insert(approval_id.as_str())
                            && let Some(tool) = upstream_tools.get(approval_id)
                        {
                            out.push(TimelineComponent::Tool(tool.clone()));
                        }
                    }
                    other => pending.push(other.clone()),
                }
            }
            flush_committed_blocks(
                &mut out,
                &mut pending,
                &id,
                stop_reason,
                error_message,
                *timestamp,
                &mut thinking_index,
            );
            out
        }
        _ => Vec::new(),
    }
}

fn flush_committed_blocks(
    out: &mut Vec<TimelineComponent>,
    pending: &mut Vec<piko_protocol::ContentBlock>,
    message_id: &str,
    stop_reason: &Option<String>,
    error_message: &Option<String>,
    timestamp: Option<i64>,
    thinking_index: &mut u32,
) {
    if pending.is_empty() {
        return;
    }
    let blocks = std::mem::take(pending);
    let mut assistant_blocks = Vec::new();
    for block in blocks {
        let piko_protocol::ContentBlock::Thinking {
            thinking,
            duration_ms,
            ..
        } = block
        else {
            assistant_blocks.push(block);
            continue;
        };
        if !assistant_blocks.is_empty() {
            out.push(TimelineComponent::Assistant(AssistantMessageComponent {
                id: ComponentId::MessageId(message_id.to_string()),
                blocks: assistant_blocks.drain(..).map(ContentBlock::from).collect(),
                stop_reason: stop_reason.clone(),
                error_message: error_message.clone(),
                timestamp,
            }));
        }
        if !thinking.trim().is_empty() {
            let key = ThoughtKey {
                message_id: message_id.to_string(),
                segment_index: *thinking_index,
            };
            out.push(TimelineComponent::Thought(ThoughtComponent {
                id: ComponentId::Thought(key.clone()),
                key,
                text: thinking,
                phase: ThoughtPhase::Completed { duration_ms },
            }));
        }
        *thinking_index = (*thinking_index).saturating_add(1);
    }
    if !assistant_blocks.is_empty() {
        out.push(TimelineComponent::Assistant(AssistantMessageComponent {
            id: ComponentId::MessageId(message_id.to_string()),
            blocks: assistant_blocks
                .into_iter()
                .map(ContentBlock::from)
                .collect(),
            stop_reason: stop_reason.clone(),
            error_message: error_message.clone(),
            timestamp,
        }));
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
    let has_text_segment = draft
        .content_segments
        .iter()
        .any(|segment| segment.kind == piko_client_core::RealtimeContentKind::Text);
    let blocks: Vec<DraftBlock> = draft
        .content_segments
        .iter()
        .filter(|segment| !segment.text.is_empty())
        .map(|segment| DraftBlock {
            kind: segment.kind,
            content_index: segment.content_index,
            text: segment.text.clone(),
        })
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
            out.extend(live_components(
                &draft.message_id,
                before,
                draft.stop_reason.clone(),
                draft.error_message.clone(),
            ));
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
        out.extend(live_components(
            &draft.message_id,
            tail,
            draft.stop_reason.clone(),
            draft.error_message.clone(),
        ));
    }
    if out.is_empty() && has_text_segment {
        out.push(assistant_component(
            &draft.message_id,
            vec![ContentBlock::Text(String::new())],
            draft.stop_reason.clone(),
            draft.error_message.clone(),
        ));
    }
    out
}

#[derive(Clone)]
struct DraftBlock {
    kind: piko_client_core::RealtimeContentKind,
    content_index: u32,
    text: String,
}

fn live_components(
    message_id: &str,
    blocks: Vec<DraftBlock>,
    stop_reason: Option<String>,
    error_message: Option<String>,
) -> Vec<TimelineComponent> {
    let mut out = Vec::new();
    let mut assistant_blocks = Vec::new();
    for block in blocks {
        if block.kind == piko_client_core::RealtimeContentKind::Thinking {
            if !assistant_blocks.is_empty() {
                out.push(assistant_component(
                    message_id,
                    std::mem::take(&mut assistant_blocks),
                    stop_reason.clone(),
                    error_message.clone(),
                ));
            }
            let key = ThoughtKey {
                message_id: message_id.to_string(),
                segment_index: block.content_index,
            };
            out.push(TimelineComponent::Thought(ThoughtComponent {
                id: ComponentId::Thought(key.clone()),
                key,
                text: block.text,
                phase: ThoughtPhase::Streaming {
                    observed_at: Instant::now(),
                },
            }));
        } else {
            assistant_blocks.push(ContentBlock::Text(block.text));
        }
    }
    if !assistant_blocks.is_empty() {
        out.push(assistant_component(
            message_id,
            assistant_blocks,
            stop_reason,
            error_message,
        ));
    }
    out
}

fn assistant_component(
    message_id: &str,
    blocks: Vec<ContentBlock>,
    stop_reason: Option<String>,
    error_message: Option<String>,
) -> TimelineComponent {
    TimelineComponent::Assistant(AssistantMessageComponent {
        id: ComponentId::MessageId(message_id.to_string()),
        blocks,
        stop_reason,
        error_message,
        timestamp: None,
    })
}

/// Clip the interleaved text/thinking blocks to `[text_lo..text_hi)` in the
/// text stream and `[think_lo..think_hi)` in the thinking stream, preserving
/// the original segment order.
fn clip_blocks(
    blocks: &[DraftBlock],
    text_lo: usize,
    text_hi: usize,
    think_lo: usize,
    think_hi: usize,
) -> Vec<DraftBlock> {
    let mut out = Vec::new();
    let mut text_pos = 0usize;
    let mut think_pos = 0usize;
    for block in blocks {
        let len = block.text.chars().count();
        let (seg_start, seg_end, lo, hi) = match block.kind {
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
            out.push(DraftBlock {
                kind: block.kind,
                content_index: block.content_index,
                text: char_slice(&block.text, take_start - seg_start, take_end - seg_start),
            });
        }
        if matches!(block.kind, piko_client_core::RealtimeContentKind::Text) {
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
#[path = "projection_tests.rs"]
mod tests;
