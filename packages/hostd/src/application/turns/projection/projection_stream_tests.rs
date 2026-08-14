use piko_protocol::agent_runtime::{RealtimeDelta, RealtimeDeltaEnvelope};

use super::super::stream_items_from_delta;

#[test]
fn stream_projection_preserves_message_identity_and_delta_seq() {
    let events = stream_items_from_delta(
        "session-1",
        &RealtimeDeltaEnvelope {
            agent_instance_id: "root".into(),
            execution_id: "exec-1".into(),
            agent_id: "main".into(),
            message_id: Some("message-1".into()),
            delta_seq: 7,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: "hello".into(),
            },
        },
    );
    assert_eq!(events.len(), 1);
    let crate::api::ServerMessage::StreamItem(patch) = &events[0] else {
        panic!("expected StreamItem");
    };
    assert_eq!(patch.session_id.as_deref(), Some("session-1"));
    assert_eq!(patch.item_id, "message-1");
    assert_eq!(patch.delta_seq, Some(7));
    assert_eq!(patch.text.as_deref(), Some("hello"));
}

#[test]
fn stream_projection_rejects_missing_message_identity() {
    assert!(
        stream_items_from_delta(
            "session-1",
            &RealtimeDeltaEnvelope {
                agent_instance_id: "root".into(),
                execution_id: "exec-1".into(),
                agent_id: "main".into(),
                message_id: None,
                delta_seq: 0,
                delta: RealtimeDelta::MessageStarted {
                    role: piko_protocol::MessageRole::Assistant,
                },
            },
        )
        .is_empty()
    );
}
