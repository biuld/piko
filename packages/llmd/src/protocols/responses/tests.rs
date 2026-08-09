use serde_json::json;

use super::*;
use crate::checkpoint::{ConversationPlan, encode as encode_checkpoint, plan};
use crate::gateway::{FinishReason, InferenceEvent, InferenceItem};
use crate::modeling::{ProtocolProfile, ResponsesContinuationPolicy};
use crate::protocols::{ProtocolAdapter, ProtocolStream};
use crate::target::{ModelTarget, ModelTargetConfig};

fn target_with_policy(policy: ResponsesContinuationPolicy) -> ModelTarget {
    let mut config = ModelTargetConfig::new(
        "fixture/gpt@responses",
        "responses",
        piko_protocol::model::ProviderAuthMethod::ApiKey,
        ProtocolProfile::Responses {
            continuation: policy,
        },
    );
    config.base_url = Some("https://example.test/v1".into());
    ModelTarget::resolve("fixture/gpt", "gpt", &config, None).unwrap()
}

fn target() -> ModelTarget {
    target_with_policy(ResponsesContinuationPolicy::PreviousResponseId)
}

fn request_with_checkpoint(target: &ModelTarget) -> crate::gateway::InferenceRequest {
    let mut request = crate::protocols::tests_support::semantic_request();
    let prefix = crate::gateway::Conversation {
        instructions: request.conversation.instructions.clone(),
        items: request.conversation.items[..1].to_vec(),
    };
    let checkpoint = encode_checkpoint(
        target,
        &prefix,
        crate::checkpoint::assistant_output_digest("", "calling"),
        json!({
            "response_id":"resp_previous",
            "output_item_ids":[],
            "call_ids":[],
            "encrypted_reasoning":[]
        }),
    )
    .unwrap();
    request.conversation.items[1].checkpoint = Some(checkpoint);
    request
}

#[test]
fn valid_checkpoint_resumes_with_only_uncovered_suffix() {
    let target = target();
    let request = request_with_checkpoint(&target);
    let plan = plan(&target, &request.conversation).unwrap();
    let ConversationPlan::Resume { suffix, .. } = &plan else {
        panic!("expected resume plan");
    };
    assert_eq!(suffix.len(), request.conversation.items.len() - 2);
    let body = ResponsesAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert_eq!(body["previous_response_id"], "resp_previous");
    assert_eq!(body["input"].as_array().unwrap().len(), suffix.len());
}

#[test]
fn responses_encodes_the_same_typed_controls() {
    let mut request = crate::protocols::tests_support::semantic_request();
    request.options.tool_choice = crate::gateway::ToolChoice::Required;
    request.options.parallel_tools = Some(false);
    request.options.max_output_tokens = Some(321);
    request.options.structured_output = Some(crate::gateway::StructuredOutputIntent {
        schema: json!({"type":"object"}),
        strict: false,
    });
    let mut target = target();
    target.capabilities.structured_json_schema = true;
    let plan = plan(&target, &request.conversation).unwrap();
    let body = ResponsesAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert_eq!(body["parallel_tool_calls"], false);
    assert_eq!(body["tool_choice"], "required");
    assert_eq!(body["max_output_tokens"], 321);
    assert_eq!(body["text"]["format"]["type"], "json_schema");
}

#[test]
fn malformed_or_wrong_target_checkpoint_falls_back_to_full_replay() {
    let target = target();
    let mut request = request_with_checkpoint(&target);
    let malformed = serde_json::from_value(json!("not-base64")).unwrap();
    request.conversation.items[1].checkpoint = Some(malformed);
    assert!(matches!(
        plan(&target, &request.conversation).unwrap(),
        ConversationPlan::FullReplay { .. }
    ));

    let other = target_with_policy(ResponsesContinuationPolicy::StatelessReplay);
    let request = request_with_checkpoint(&target);
    assert!(matches!(
        plan(&other, &request.conversation).unwrap(),
        ConversationPlan::FullReplay { .. }
    ));
}

#[test]
fn complete_response_emits_opaque_checkpoint_and_semantic_ids() {
    let request = crate::protocols::tests_support::semantic_request();
    let result = ResponsesAdapter
        .decode_response(
            json!({
                "id":"resp_1","status":"completed","output":[
                    {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"why"}]},
                    {"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{}"},
                    {"type":"message","id":"msg_1","content":[{"type":"output_text","text":"done"}]}
                ],"usage":{"input_tokens":4,"output_tokens":3}
            }),
            &target(),
            &request,
        )
        .unwrap();
    assert!(result.checkpoint.is_some());
    assert!(matches!(
        result.finish_reason,
        FinishReason::Completed { .. }
    ));
    assert!(
        matches!(&result.items[0], InferenceItem::Reasoning { id, .. } if id.0.starts_with("out_") && !id.0.contains("rs_1"))
    );
    assert!(
        matches!(&result.items[1], InferenceItem::ToolCall { call_id, .. } if call_id.0 == "call_1")
    );
    assert!(
        matches!(&result.items[2], InferenceItem::Text { id, .. } if id.0.starts_with("out_") && !id.0.contains("msg_1"))
    );
}

#[test]
fn stream_checkpoint_precedes_terminal_event() {
    let target = target();
    let request = crate::protocols::tests_support::semantic_request();
    let mut stream = ResponsesStream::new(target, request);
    stream
        .push(json!({"type":"response.created","response":{"id":"resp_1"}}))
        .unwrap();
    stream
        .push(json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_1"}}))
        .unwrap();
    let delta = stream
        .push(json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"done"}))
        .unwrap();
    assert!(
        matches!(&delta[0], InferenceEvent::TextDelta { item_id, .. } if item_id.0.starts_with("out_") && !item_id.0.contains("msg_1"))
    );
    let terminal = stream
        .push(json!({"type":"response.completed","response":{"id":"resp_1","usage":{"input_tokens":1,"output_tokens":1}}}))
        .unwrap();
    assert!(matches!(terminal[1], InferenceEvent::Checkpoint(_)));
    assert!(matches!(terminal[2], InferenceEvent::Completed(_)));
}

#[test]
fn unknown_required_output_fails_closed() {
    let request = crate::protocols::tests_support::semantic_request();
    assert!(ResponsesAdapter
        .decode_response(
            json!({"id":"resp_1","status":"completed","output":[{"type":"future_required","id":"x"}]}),
            &target(),
            &request,
        )
        .is_err());
}

#[test]
fn incomplete_response_never_emits_a_durable_checkpoint() {
    let request = crate::protocols::tests_support::semantic_request();
    let result = ResponsesAdapter
        .decode_response(
            json!({
                "id":"resp_incomplete","status":"incomplete","output":[],
                "incomplete_details":{"reason":"max_output_tokens"}
            }),
            &target(),
            &request,
        )
        .unwrap();
    assert!(result.checkpoint.is_none());
    assert!(matches!(
        result.finish_reason,
        FinishReason::Incomplete { .. }
    ));
}

#[tokio::test]
async fn streaming_and_assembled_delivery_have_equivalent_semantics() {
    let request = crate::protocols::tests_support::semantic_request();
    let target = target();
    let direct = ResponsesAdapter
        .decode_response(
            json!({
                "id":"resp_equal","status":"completed",
                "output":[{"type":"message","id":"msg_equal","content":[
                    {"type":"output_text","text":"same output"}
                ]}],
                "usage":{"input_tokens":7,"output_tokens":2}
            }),
            &target,
            &request,
        )
        .unwrap();

    let mut decoder = ResponsesStream::new(target, request);
    let mut events = Vec::new();
    for value in [
        json!({"type":"response.created","response":{"id":"resp_equal"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_equal"}}),
        json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"same output"}),
        json!({"type":"response.completed","response":{"id":"resp_equal","usage":{"input_tokens":7,"output_tokens":2}}}),
    ] {
        events.extend(decoder.push(value).unwrap());
    }
    let streamed = crate::gateway::collect_execution(crate::gateway::InferenceExecution {
        events: Box::pin(futures::stream::iter(events)),
        handle: None,
    })
    .await
    .unwrap();

    assert_eq!(streamed.items, direct.items);
    assert_eq!(streamed.usage, direct.usage);
    assert_eq!(streamed.finish_reason, direct.finish_reason);
    assert_eq!(streamed.checkpoint, direct.checkpoint);
    assert_eq!(streamed.auxiliary, direct.auxiliary);
}
