use piko_llmd::gateway::{
    GeneratedArtifact, InferenceCitation, InferenceEvent, InferenceSource, OutputItemId,
    SemanticResourceRef, UpstreamApprovalRequest, UpstreamToolActivity,
};
use piko_llmd::tools::UpstreamActivityStatus;
use piko_protocol::Message;

use super::*;

#[test]
fn llmd_checkpoint_is_persisted_without_interpretation() {
    let mut state = AssistantMessageState::new();
    let checkpoint: piko_protocol::OpaqueModelCheckpoint =
        serde_json::from_value(serde_json::json!("opaque-token")).unwrap();
    state.apply_gateway_event(&InferenceEvent::Checkpoint(checkpoint.clone()));

    let message = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    });

    assert!(matches!(
        message,
        Message::Assistant {
            checkpoint: Some(persisted),
            ..
        } if persisted.as_ref() == &checkpoint
    ));
}

#[test]
fn stateless_terminal_requires_no_checkpoint() {
    let mut state = AssistantMessageState::new();
    state.apply_gateway_event(&InferenceEvent::completed("stop"));

    let message = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    });
    assert!(matches!(
        message,
        Message::Assistant {
            checkpoint: None,
            stop_reason: Some(reason),
            error_message: None,
            ..
        } if reason == "stop"
    ));
}

#[test]
fn incomplete_terminal_discards_a_pending_checkpoint() {
    let mut state = AssistantMessageState::new();
    let checkpoint = serde_json::from_value(serde_json::json!("opaque-token")).unwrap();
    state.apply_gateway_event(&InferenceEvent::Checkpoint(checkpoint));
    state.apply_gateway_event(&InferenceEvent::Completed(FinishReason::Cancelled));
    let message = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    });
    assert!(matches!(
        message,
        Message::Assistant {
            checkpoint: None,
            ..
        }
    ));
}

#[test]
fn thinking_runs_record_monotonic_durations_and_ordered_indices() {
    let start = Instant::now();
    let mut state = AssistantMessageState::new();
    state.apply_gateway_event_at(
        &InferenceEvent::ReasoningDelta {
            item_id: OutputItemId("reasoning-1".into()),
            delta: "first".into(),
        },
        start,
    );
    state.apply_gateway_event_at(
        &InferenceEvent::ReasoningDelta {
            item_id: OutputItemId("reasoning-1".into()),
            delta: " thought".into(),
        },
        start + std::time::Duration::from_millis(100),
    );
    assert_eq!(state.current_thinking_index(), Some(0));
    state.apply_gateway_event_at(
        &InferenceEvent::TextDelta {
            item_id: OutputItemId("text-1".into()),
            delta: "answer".into(),
        },
        start + std::time::Duration::from_millis(250),
    );
    assert_eq!(state.current_thinking_index(), None);
    state.apply_gateway_event_at(
        &InferenceEvent::ReasoningDelta {
            item_id: OutputItemId("reasoning-2".into()),
            delta: "second".into(),
        },
        start + std::time::Duration::from_millis(500),
    );
    assert_eq!(state.current_thinking_index(), Some(1));
    state.apply_gateway_event_at(
        &InferenceEvent::ToolCallDelta {
            call_id: piko_llmd::gateway::ToolCallId("call-1".into()),
            name: "search".into(),
            arguments_delta: "{}".into(),
        },
        start + std::time::Duration::from_millis(900),
    );
    state.apply_gateway_event_at(
        &InferenceEvent::Completed(FinishReason::Completed {
            reason: "stop".into(),
        }),
        start + std::time::Duration::from_millis(1000),
    );

    let Message::Assistant { content, .. } = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    }) else {
        panic!("expected assistant message");
    };
    assert!(matches!(
        &content[0],
        ContentBlock::Thinking {
            thinking,
            duration_ms: Some(250),
            ..
        } if thinking == "first thought"
    ));
    assert!(matches!(&content[1], ContentBlock::Text { text } if text == "answer"));
    assert!(matches!(
        &content[2],
        ContentBlock::Thinking {
            thinking,
            duration_ms: Some(400),
            ..
        } if thinking == "second"
    ));
}

#[test]
fn cancellation_finalizes_an_open_thinking_run() {
    let start = Instant::now();
    let mut state = AssistantMessageState::new();
    state.apply_gateway_event_at(
        &InferenceEvent::ReasoningDelta {
            item_id: OutputItemId("reasoning-1".into()),
            delta: "cancelled thought".into(),
        },
        start,
    );
    state.apply_gateway_event_at(
        &InferenceEvent::Completed(FinishReason::Cancelled),
        start + std::time::Duration::from_millis(123),
    );
    let Message::Assistant { content, .. } = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    }) else {
        panic!("expected assistant message");
    };
    assert!(matches!(
        &content[0],
        ContentBlock::Thinking {
            duration_ms: Some(123),
            ..
        }
    ));
}

#[test]
fn upstream_observations_are_projected_without_orchd_policy_decisions() {
    let mut state = AssistantMessageState::new();
    for event in [
        InferenceEvent::UpstreamActivity(UpstreamToolActivity {
            activity_id: "activity-1".into(),
            tool_name: "search".into(),
            kind: piko_llmd::capabilities::UpstreamToolKind::new("search").unwrap(),
            status: UpstreamActivityStatus::InProgress,
            arguments: None,
            action: None,
        }),
        InferenceEvent::ApprovalRequired(UpstreamApprovalRequest {
            approval_id: "approval-1".into(),
            tool_name: "search".into(),
            summary: "search the web".into(),
        }),
        InferenceEvent::Source(InferenceSource {
            source_id: "source-1".into(),
            title: Some("Source".into()),
            uri: Some("https://example.test".into()),
        }),
        InferenceEvent::Citation(InferenceCitation {
            source_id: "source-1".into(),
            output_item_id: OutputItemId("out_semantic".into()),
            start: Some(0),
            end: Some(4),
        }),
        InferenceEvent::Artifact(GeneratedArtifact {
            artifact_id: "artifact-1".into(),
            media_type: "image/png".into(),
            resource: SemanticResourceRef {
                namespace: "session".into(),
                resource: "artifact-1".into(),
            },
        }),
    ] {
        state.apply_gateway_event(&event);
    }

    assert_eq!(state.semantic_blocks.len(), 5);
    assert!(state.error_message.is_none());
    assert!(matches!(
        &state.semantic_blocks[1],
        ContentBlock::UpstreamToolApproval { approval_id, .. } if approval_id == "approval-1"
    ));
}

#[test]
fn upstream_lifecycle_collapses_to_single_block_by_activity_id() {
    let mut state = AssistantMessageState::new();
    let activity = |status: UpstreamActivityStatus| {
        InferenceEvent::UpstreamActivity(UpstreamToolActivity {
            activity_id: "ws_1".into(),
            tool_name: "web_search".into(),
            kind: piko_llmd::capabilities::UpstreamToolKind::new("search").unwrap(),
            status,
            arguments: if matches!(status, UpstreamActivityStatus::Completed) {
                Some(serde_json::json!({ "query": "USD CNY" }))
            } else {
                None
            },
            action: None,
        })
    };
    state.apply_gateway_event(&activity(UpstreamActivityStatus::InProgress));
    state.apply_gateway_event(&activity(UpstreamActivityStatus::Completed));

    // One block per activity_id (latest state wins), not one per event.
    let upstream_blocks = state
        .semantic_blocks
        .iter()
        .filter(|b| matches!(b, ContentBlock::UpstreamToolActivity { .. }))
        .count();
    assert_eq!(upstream_blocks, 1, "lifecycle events collapse to one block");

    let message = state.build_message(&ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    });
    let Message::Assistant { content, .. } = &message else {
        panic!("expected assistant message");
    };
    let upstream_blocks = content
        .iter()
        .filter(|b| matches!(b, ContentBlock::UpstreamToolActivity { .. }))
        .count();
    assert_eq!(upstream_blocks, 1, "committed content collapses upstream");
    assert!(content.iter().any(|b| matches!(
        b,
        ContentBlock::UpstreamToolActivity {
            status: piko_protocol::messages::UpstreamActivityStatus::Completed,
            arguments: Some(arguments),
            ..
        } if arguments.get("query").and_then(|q| q.as_str()) == Some("USD CNY")
    )));
}
