use super::{RealtimeContentKind, RealtimeContentSegment};
use piko_protocol::StreamItemOp;

pub(super) fn update_content_segment(
    segments: &mut Vec<RealtimeContentSegment>,
    kind: RealtimeContentKind,
    content_index: u32,
    op: StreamItemOp,
    text: Option<&str>,
) {
    let position = segments
        .iter()
        .position(|segment| segment.kind == kind && segment.content_index == content_index);
    match op {
        StreamItemOp::AppendChunk | StreamItemOp::ReplaceContent | StreamItemOp::Upsert => {
            let position = position.unwrap_or_else(|| {
                segments.push(RealtimeContentSegment {
                    kind,
                    content_index,
                    text: String::new(),
                });
                segments.len() - 1
            });
            if op == StreamItemOp::AppendChunk {
                segments[position].text.push_str(text.unwrap_or_default());
            } else {
                segments[position].text = text.unwrap_or_default().to_string();
            }
        }
        StreamItemOp::ClearContent => {
            if let Some(position) = position {
                segments[position].text.clear();
            }
        }
    }
}

pub(super) fn replace_kind_content(
    segments: &mut Vec<RealtimeContentSegment>,
    kind: RealtimeContentKind,
    content_index: u32,
    text: Option<&str>,
) {
    let insert_at = segments
        .iter()
        .position(|segment| segment.kind == kind)
        .unwrap_or(segments.len());
    segments.retain(|segment| segment.kind != kind);
    segments.insert(
        insert_at.min(segments.len()),
        RealtimeContentSegment {
            kind,
            content_index,
            text: text.unwrap_or_default().to_string(),
        },
    );
}

pub(super) fn clear_kind_content(
    segments: &mut Vec<RealtimeContentSegment>,
    kind: RealtimeContentKind,
) {
    segments.retain(|segment| segment.kind != kind);
}
