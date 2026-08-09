use piko_llmd::gateway::Conversation;
use piko_protocol::{ContentBlock, Message, SemanticRunPrompt};

fn assistant(text: &str, timestamp: i64, checkpoint: Option<&str>, provider: &str) -> Message {
    Message::Assistant {
        content: vec![ContentBlock::Text { text: text.into() }],
        checkpoint: checkpoint
            .map(|token| Box::new(serde_json::from_value(serde_json::json!(token)).unwrap())),
        provider: provider.into(),
        model: "presentation-model".into(),
        usage: None,
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: Some(timestamp),
    }
}

#[test]
fn semantic_ids_ignore_persistence_and_provider_metadata() {
    let left = Conversation::from_messages(
        SemanticRunPrompt::default(),
        vec![assistant("same", 1, Some("checkpoint-a"), "provider-a")],
    );
    let right = Conversation::from_messages(
        SemanticRunPrompt::default(),
        vec![assistant("same", 999, Some("checkpoint-b"), "provider-b")],
    );
    assert_eq!(left.items[0].id, right.items[0].id);
    assert_ne!(left.items[0].checkpoint, right.items[0].checkpoint);

    let changed = Conversation::from_messages(
        SemanticRunPrompt::default(),
        vec![assistant("changed", 1, None, "provider-a")],
    );
    assert_ne!(left.items[0].id, changed.items[0].id);
}

#[test]
fn duplicate_adjacent_items_still_receive_distinct_prefix_ids() {
    let conversation = Conversation::from_messages(
        SemanticRunPrompt::default(),
        vec![
            assistant("duplicate", 1, None, "provider"),
            assistant("duplicate", 1, None, "provider"),
        ],
    );
    assert_ne!(conversation.items[0].id, conversation.items[1].id);
}
