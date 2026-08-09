use serde_json::json;

use super::*;
use crate::checkpoint::{ConversationPlan, plan};
use crate::gateway::{FinishReason, InferenceEvent, InferenceItem};
use crate::modeling::ProtocolProfile;
use crate::protocols::ProtocolAdapter;
use crate::target::{ModelTarget, ModelTargetConfig};

fn target() -> ModelTarget {
    let mut config = ModelTargetConfig::new(
        "fixture/gpt@chat",
        "chat",
        piko_protocol::model::ProviderAuthMethod::ApiKey,
        ProtocolProfile::ChatCompletions,
    );
    config.base_url = Some("https://example.test/v1".into());
    ModelTarget::resolve("fixture/gpt", "gpt", &config, None).unwrap()
}

#[test]
fn full_replay_encodes_one_neutral_request_without_checkpoint() {
    let request = crate::protocols::tests_support::semantic_request();
    let target = target();
    let plan = plan(&target, &request.conversation).unwrap();
    assert!(matches!(plan, ConversationPlan::FullReplay { .. }));
    let body = ChatCompletionsAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 5);
    assert!(body.get("previous_response_id").is_none());
}

#[test]
fn typed_controls_are_encoded_without_provider_option_maps() {
    let mut request = crate::protocols::tests_support::semantic_request();
    request.options.tool_choice = crate::gateway::ToolChoice::Specific("read".into());
    request.options.parallel_tools = Some(false);
    request.options.max_output_tokens = Some(321);
    request.options.structured_output = Some(crate::gateway::StructuredOutputIntent {
        schema: json!({"type":"object","required":["answer"]}),
        strict: true,
    });
    let mut target = target();
    target.capabilities.structured_json_schema = true;
    target.capabilities.strict_structured_output = true;
    let plan = plan(&target, &request.conversation).unwrap();
    let body = ChatCompletionsAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert_eq!(body["parallel_tool_calls"], false);
    assert_eq!(body["tool_choice"]["function"]["name"], "read");
    assert_eq!(body["max_completion_tokens"], 321);
    assert_eq!(body["response_format"]["type"], "json_schema");
}

#[test]
fn complete_response_has_only_semantic_identities() {
    let request = crate::protocols::tests_support::semantic_request();
    let result = ChatCompletionsAdapter
        .decode_response(
            json!({
                "choices":[{"index":0,"message":{"content":"done","tool_calls":[
                    {"id":"call_1","function":{"name":"read","arguments":"{}"}}
                ]},"finish_reason":"tool_calls"}],
                "usage":{"prompt_tokens":5,"completion_tokens":4}
            }),
            &target(),
            &request,
        )
        .unwrap();
    assert!(
        matches!(result.finish_reason, FinishReason::Completed { ref reason } if reason == "tool_calls")
    );
    assert!(result.checkpoint.is_none());
    assert!(
        matches!(&result.items[0], InferenceItem::Text { id, .. } if id.0.starts_with("out_") && !id.0.contains("msg"))
    );
    assert!(
        matches!(&result.items[1], InferenceItem::ToolCall { call_id, .. } if call_id.0 == "call_1")
    );
}

#[test]
fn stream_uses_stable_semantic_call_id() {
    let request = crate::protocols::tests_support::semantic_request();
    let mut stream = ChatCompletionsAdapter.new_stream(&target(), &request);
    let events = stream
        .push(json!({"choices":[{"index":0,"delta":{"tool_calls":[{
            "index":0,"id":"call_a","function":{"name":"read","arguments":"{}"}
        }]},"finish_reason":"tool_calls"}]}))
        .unwrap();
    assert!(
        matches!(&events[0], InferenceEvent::ToolCallDelta { call_id, .. } if call_id.0 == "call_a")
    );
    assert!(matches!(
        &stream.finish().unwrap()[0],
        InferenceEvent::Completed(_)
    ));
}

#[tokio::test]
async fn streaming_and_assembled_chat_delivery_are_equivalent() {
    let request = crate::protocols::tests_support::semantic_request();
    let target = target();
    let direct = ChatCompletionsAdapter
        .decode_response(
            json!({
                "choices":[{"index":0,"message":{"content":"same output"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":7,"completion_tokens":2}
            }),
            &target,
            &request,
        )
        .unwrap();
    let mut decoder = ChatStream::new(target.id.clone(), request);
    let mut events = decoder
        .push(json!({"choices":[{"index":0,"delta":{"content":"same output"},"finish_reason":"stop"}]}))
        .unwrap();
    events.extend(
        decoder
            .push(json!({"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":2}}))
            .unwrap(),
    );
    events.extend(decoder.finish().unwrap());
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
}
