use super::*;
use crate::gateway::{ModelEvent, TerminalStatus};
use crate::modeling::{ProtocolProfile, ResponsesContinuationPolicy};
use crate::protocols::{ProtocolAdapter, ProtocolStream};
use crate::target::{ModelTarget, ModelTargetConfig};
use piko_protocol::model::ProviderAuthMethod;
use piko_protocol::{ContentBlock, Message, ModelContinuation};
use serde_json::json;

fn continuation(
    response_id: &str,
    encrypted_reasoning: Vec<super::support::EncryptedReasoningItem>,
) -> ModelContinuation {
    ModelContinuation {
        adapter: crate::modeling::ProtocolKind::Responses.adapter_id().into(),
        state: serde_json::to_value(super::support::ResponsesContinuation {
            response_id: response_id.into(),
            output_item_ids: vec!["msg_previous".into()],
            call_ids: vec!["call_a".into(), "call_b".into()],
            encrypted_reasoning,
        })
        .unwrap(),
    }
}

fn target_with_policy(continuation: ResponsesContinuationPolicy) -> ModelTarget {
    ModelTarget::resolve(
        "fixture",
        "gpt",
        &{
            let mut config = ModelTargetConfig::new(
                "fixture/gpt@platform",
                "platform",
                ProviderAuthMethod::ApiKey,
                ProtocolProfile::Responses { continuation },
            );
            config.base_url = Some("https://example.test/v1".into());
            config
        },
        None,
    )
    .unwrap()
}

fn target() -> ModelTarget {
    target_with_policy(ResponsesContinuationPolicy::PreviousResponseId)
}

#[test]
fn request_fixture_preserves_instructions_reasoning_tools_calls_and_outputs() {
    let body = ResponsesAdapter
        .encode(
            &crate::protocols::tests_support::semantic_request(),
            &target(),
            true,
        )
        .unwrap();
    assert!(body["instructions"].as_str().unwrap().ends_with("be exact"));
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["tools"][0]["name"], "read");
    assert_eq!(body["parallel_tool_calls"], true);
    assert!(
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "function_call" && item["call_id"] == "call_a")
    );
    assert!(
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "function_call_output" && item["call_id"] == "call_b")
    );
}

#[test]
fn retained_response_continuation_sends_only_the_transcript_suffix() {
    let mut request = crate::protocols::tests_support::semantic_request();
    let Message::Assistant { continuation, .. } = &mut request.transcript[1] else {
        panic!("fixture assistant missing");
    };
    *continuation = Some(Box::new(self::continuation("resp_previous", Vec::new())));

    let body = ResponsesAdapter.encode(&request, &target(), true).unwrap();
    assert_eq!(body["previous_response_id"], "resp_previous");
    assert_eq!(body["store"], true);
    let input = body["input"].as_array().unwrap();
    assert_eq!(input.len(), request.transcript.len() - 2);
    assert!(input.iter().all(|item| item.get("role").is_none()));
}

#[test]
fn reasoning_replay_without_continuation_fails_instead_of_dropping_state() {
    let mut request = crate::protocols::tests_support::semantic_request();
    let Message::Assistant { content, .. } = &mut request.transcript[1] else {
        panic!("fixture assistant missing");
    };
    content.insert(
        0,
        ContentBlock::Thinking {
            thinking: "private state".into(),
            thinking_signature: None,
        },
    );
    assert!(ResponsesAdapter.encode(&request, &target(), true).is_err());
}

#[test]
fn encrypted_reasoning_policy_replays_opaque_state_without_server_storage() {
    let mut request = crate::protocols::tests_support::semantic_request();
    let Message::Assistant {
        content,
        continuation,
        ..
    } = &mut request.transcript[1]
    else {
        panic!("fixture assistant missing");
    };
    content.insert(
        0,
        ContentBlock::Thinking {
            thinking: "summary".into(),
            thinking_signature: None,
        },
    );
    *continuation = Some(Box::new(self::continuation(
        "resp_previous",
        vec![super::support::EncryptedReasoningItem {
            item_id: "rs_previous".into(),
            encrypted_content: "opaque".into(),
        }],
    )));
    let target = target_with_policy(ResponsesContinuationPolicy::EncryptedReasoning);

    let body = ResponsesAdapter.encode(&request, &target, true).unwrap();
    assert_eq!(body["store"], false);
    assert!(body.get("previous_response_id").is_none());
    assert_eq!(body["include"][0], "reasoning.encrypted_content");
    assert!(
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["type"] == "reasoning" && item["encrypted_content"] == "opaque" })
    );
}

#[test]
fn stateless_policy_replays_full_history_and_plaintext_reasoning() {
    let mut request = crate::protocols::tests_support::semantic_request();
    let Message::Assistant { content, .. } = &mut request.transcript[1] else {
        panic!("fixture assistant missing");
    };
    content.insert(
        0,
        ContentBlock::Thinking {
            thinking: "plain reasoning".into(),
            thinking_signature: None,
        },
    );
    let target = target_with_policy(ResponsesContinuationPolicy::StatelessReplay);

    let body = ResponsesAdapter.encode(&request, &target, true).unwrap();
    assert!(body.get("store").is_none());
    assert!(body.get("include").is_none());
    assert!(body.get("previous_response_id").is_none());
    assert!(body["input"].as_array().is_some_and(|input| {
        input.len() == request.transcript.len() + 1
            && input.iter().any(|item| {
                item["type"] == "reasoning"
                    && item["content"][0]["type"] == "reasoning_text"
                    && item["content"][0]["text"] == "plain reasoning"
            })
    }));
}

#[test]
fn plaintext_reasoning_response_and_done_event_decode() {
    let result = ResponsesAdapter
        .decode_response(
            json!({
                "id":"resp_deepseek","status":"completed","output":[
                    {"type":"reasoning","id":"rs_1","content":[
                        {"type":"reasoning_text","text":"plain reasoning"}
                    ]},
                    {"type":"message","id":"msg_1","role":"assistant","content":[
                        {"type":"output_text","text":"answer"}
                    ]}
                ]
            }),
            &target(),
        )
        .unwrap();
    assert!(result.items.iter().any(|item| matches!(
        item,
        crate::gateway::SemanticItem::Reasoning { text, .. } if text == "plain reasoning"
    )));

    let mut stream = ResponsesStream::new("deepseek/deepseek-v4-flash".into());
    assert!(
        stream
            .push(json!({"type":"response.created","response":{"id":"resp_1"}}))
            .is_ok()
    );
    assert!(
        stream
            .push(json!({
                "type":"response.output_item.added","output_index":0,
                "item":{"type":"reasoning","id":"rs_1"}
            }))
            .is_ok()
    );
    assert!(
        stream
            .push(json!({
                "type":"response.reasoning_text.done","output_index":0,
                "item_id":"rs_1","content_index":0,"text":"plain reasoning"
            }))
            .is_ok()
    );
}

#[test]
fn non_streaming_preserves_response_item_and_call_identity() {
    let result = ResponsesAdapter.decode_response(json!({
        "id":"resp_1","status":"completed","output":[
            {"type":"reasoning","id":"rs_1","encrypted_content":"opaque","summary":[{"type":"summary_text","text":"why"}]},
            {"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{\"path\":\"a\"}"},
            {"type":"function_call","id":"fc_2","call_id":"call_2","name":"read","arguments":"{\"path\":\"b\"}"},
            {"type":"message","id":"msg_1","role":"assistant","content":[{"type":"output_text","text":"done"}]}
        ],"usage":{"input_tokens":10,"output_tokens":5,"input_tokens_details":{"cached_tokens":2}}
    }), &target()).unwrap();
    assert_eq!(result.usage.unwrap().cache_read, 2);
    let continuation = super::support::decode_continuation(
        result.output_metadata.continuation.as_ref().unwrap(),
        &target(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(continuation.response_id, "resp_1");
    assert_eq!(continuation.call_ids, ["call_1", "call_2"]);
}

#[test]
fn stream_rejects_premature_eof_and_duplicate_terminal() {
    let mut stream = ResponsesStream::new("fixture".into());
    stream
        .push(json!({"type":"response.created","response":{"id":"resp_1"}}))
        .unwrap();
    assert!(stream.finish().is_err());
    stream
        .push(json!({"type":"response.completed","response":{"id":"resp_1"}}))
        .unwrap();
    assert!(
        stream
            .push(json!({"type":"response.completed","response":{"id":"resp_1"}}))
            .is_err()
    );
}

#[test]
fn streaming_and_non_streaming_fixtures_are_semantically_equivalent() {
    let complete = ResponsesAdapter.decode_response(json!({
        "id":"resp_1","status":"completed","output":[
            {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"why"}]},
            {"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{}"},
            {"type":"message","id":"msg_1","content":[{"type":"output_text","text":"done"}]}
        ],"usage":{"input_tokens":4,"output_tokens":3}
    }), &target()).unwrap();
    let mut stream = ResponsesStream::new("fixture".into());
    let mut events = Vec::new();
    for event in [
        json!({"type":"response.created","response":{"id":"resp_1"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","encrypted_content":"opaque"}}),
        json!({"type":"response.reasoning_summary_text.delta","output_index":0,"delta":"why"}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read"}}),
        json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"{}"}),
        json!({"type":"response.output_item.added","output_index":2,"item":{"type":"message","id":"msg_1"}}),
        json!({"type":"response.output_text.delta","output_index":2,"content_index":0,"delta":"done"}),
        json!({"type":"response.completed","response":{"id":"resp_1","usage":{"input_tokens":4,"output_tokens":3}}}),
    ] {
        events.extend(stream.push(event).unwrap());
    }
    stream.finish().unwrap();
    assert!(events.iter().any(|event| matches!(event, ModelEvent::ReasoningDelta { delta, identity } if delta == "why" && identity.item_id.as_deref() == Some("rs_1"))));
    assert!(events.iter().any(|event| {
        let ModelEvent::OutputMetadata(metadata) = event else {
            return false;
        };
        metadata.continuation.as_ref().is_some_and(|envelope| {
            super::support::decode_continuation(envelope, &target())
                .ok()
                .flatten()
                .is_some_and(|state| {
                    state.encrypted_reasoning
                        == [super::support::EncryptedReasoningItem {
                            item_id: "rs_1".into(),
                            encrypted_content: "opaque".into(),
                        }]
                })
        })
    }));
    assert!(events.iter().any(|event| matches!(event, ModelEvent::FunctionCallDelta { name, arguments_delta, identity } if name == "read" && arguments_delta == "{}" && identity.call_id.as_deref() == Some("call_1"))));
    assert!(events.iter().any(|event| matches!(event, ModelEvent::TextDelta { delta, identity } if delta == "done" && identity.item_id.as_deref() == Some("msg_1") && identity.content_index == Some(0))));
    assert!(events.iter().any(|event| matches!(event, ModelEvent::Usage(usage) if usage == complete.usage.as_ref().unwrap())));
    assert!(
        events.iter().any(
            |event| matches!(event, ModelEvent::Completed(status) if status == &complete.status)
        )
    );
}

#[test]
fn unknown_required_event_and_item_types_fail() {
    let mut stream = ResponsesStream::new("fixture".into());
    assert!(
        stream
            .push(json!({"type":"response.future_required"}))
            .is_err()
    );
    assert!(ResponsesAdapter.decode_response(json!({
        "id":"resp_1","status":"completed","output":[{"type":"future_required","id":"x"}]
    }), &target()).is_err());

    stream
        .push(json!({"type":"response.created","response":{"id":"resp_1"}}))
        .unwrap();
    assert!(
        stream
            .push(json!({
                "type":"response.output_item.added",
                "output_index":0,
                "item":{"type":"future_required","id":"x"}
            }))
            .is_err()
    );
}

#[test]
fn terminal_response_id_must_match_created_response() {
    let mut stream = ResponsesStream::new("fixture".into());
    stream
        .push(json!({"type":"response.created","response":{"id":"resp_1"}}))
        .unwrap();
    assert!(
        stream
            .push(json!({
                "type":"response.completed",
                "response":{"id":"resp_other"}
            }))
            .is_err()
    );
}

#[test]
fn incomplete_status_is_retained() {
    let result = ResponsesAdapter.decode_response(json!({
        "id":"resp_1","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]
    }), &target()).unwrap();
    assert!(
        matches!(result.status, TerminalStatus::Incomplete { reason } if reason.as_deref() == Some("max_output_tokens"))
    );
}
