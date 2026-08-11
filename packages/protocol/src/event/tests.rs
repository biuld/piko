mod observation_projection_tests {
    use super::super::*;

    #[test]
    fn committed_and_stream_server_messages_round_trip() {
        let committed = ServerMessage::TranscriptCommitted(TranscriptCommittedEvent {
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            transcript_seq: 3,
            message: crate::Message::User {
                content: crate::MessageContent::String("hello".into()),
                timestamp: Some(1),
            },
        });
        let stream = ServerMessage::StreamItem(crate::StreamItemPatch {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("root".into()),
            item_id: "message-2".into(),
            item_kind: crate::StreamItemKind::AgentMessage,
            op: crate::StreamItemOp::AppendChunk,
            text: Some("world".into()),
            content_index: Some(0),
            delta_seq: Some(4),
            fields: Some(serde_json::json!({"parentMessageId": "message-2"})),
        });

        for event in [committed, stream] {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(event).unwrap()
            );
        }
    }

    #[test]
    fn usage_updated_round_trips() {
        let usage = ServerMessage::Usage(UsageEvent::Updated {
            session_id: "session-1".into(),
            agent_instance_id: Some("root".into()),
            turn_id: Some("turn-1".into()),
            used: 13_000,
            size: Some(128_000),
            cumulative: Some(crate::messages::Usage {
                input: 10_000,
                output: 100,
                cache_read: 3_000,
                cache_write: 0,
                total_tokens: 13_100,
                units: Default::default(),
                cost: Default::default(),
            }),
            turn_usage: Some(crate::messages::Usage {
                input: 10_000,
                output: 100,
                cache_read: 3_000,
                cache_write: 0,
                total_tokens: 13_100,
                units: Default::default(),
                cost: Default::default(),
            }),
            timestamp: 42,
        });
        let json = serde_json::to_string(&usage).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(usage).unwrap()
        );
    }

    #[test]
    fn stream_item_round_trips() {
        let patch = crate::StreamItemPatch {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("root".into()),
            item_id: "msg-1".into(),
            item_kind: crate::StreamItemKind::AgentMessage,
            op: crate::StreamItemOp::AppendChunk,
            text: Some("hi".into()),
            content_index: Some(0),
            delta_seq: Some(2),
            fields: None,
        };
        let event = ServerMessage::StreamItem(patch);
        let json = serde_json::to_string(&event).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(event).unwrap()
        );
    }

    #[test]
    fn session_entry_committed_round_trips() {
        let event = ServerMessage::SessionEntryCommitted(SessionEntryCommittedEvent {
            session_id: "session-1".into(),
            entry: crate::SessionTreeEntry::ModelChange(crate::ModelChangeEntry {
                id: "model-change".into(),
                parent_id: Some("message-1".into()),
                timestamp: "42".into(),
                provider: "openai".into(),
                model_id: "gpt".into(),
            }),
        });
        let json = serde_json::to_string(&event).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(event).unwrap()
        );
    }
}
