use super::*;
use crate::gateway::{ModelEvent, TerminalStatus};
use crate::modeling::ProtocolProfile;
use crate::protocols::{ProtocolAdapter, ProtocolStream};
use crate::target::{ModelTarget, ModelTargetConfig};
use piko_protocol::model::ProviderAuthMethod;
use serde_json::json;

fn target() -> ModelTarget {
    ModelTarget::resolve(
        "fixture",
        "gpt",
        &{
            let mut config = ModelTargetConfig::new(
                "fixture/gpt@platform",
                "platform",
                ProviderAuthMethod::ApiKey,
                ProtocolProfile::ChatCompletions,
            );
            config.base_url = Some("https://example.test/v1".into());
            config
        },
        None,
    )
    .unwrap()
}

#[test]
fn request_fixture_groups_parallel_calls_and_tool_results() {
    let body = ChatCompletionsAdapter
        .encode(
            &crate::protocols::tests_support::semantic_request(),
            &target(),
            true,
        )
        .unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    assert!(
        body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .ends_with("be exact")
    );
    assert_eq!(body["reasoning_effort"], "high");
    assert_eq!(body["tools"][0]["function"]["name"], "read");
    assert_eq!(body["parallel_tool_calls"], true);
    let assistant = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .unwrap();
    assert_eq!(assistant["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(
        body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "tool")
            .count(),
        2
    );
}

#[test]
fn non_streaming_preserves_refusal_finish_usage_and_parallel_calls() {
    let result = ChatCompletionsAdapter.decode_response(json!({
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"refusal":"no","tool_calls":[
            {"id":"call_1","type":"function","function":{"name":"a","arguments":"{}"}},
            {"id":"call_2","type":"function","function":{"name":"b","arguments":"{}"}}
        ]},"finish_reason":"tool_calls"}],
        "usage":{"prompt_tokens":7,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":2}}
    }), &target()).unwrap();
    assert!(
        matches!(result.status, TerminalStatus::Completed { ref reason } if reason == "tool_calls")
    );
    assert_eq!(result.usage.unwrap().cache_read, 2);
    let continuation = result.output_metadata.continuation.unwrap();
    assert_eq!(
        continuation.adapter,
        crate::modeling::ProtocolKind::ChatCompletions.adapter_id()
    );
    assert_eq!(continuation.state["tool_call_ids"][0], "call_1");
    assert_eq!(continuation.state["tool_call_ids"][1], "call_2");
}

#[test]
fn indexed_tool_fragments_remain_separate() {
    let mut stream = ChatStream::new("fixture".into());
    let events = stream
        .push(
            json!({"id":"chat_1","choices":[{"index":0,"delta":{"tool_calls":[
        {"index":0,"id":"call_a","function":{"name":"read","arguments":"{\"a\":"}},
        {"index":1,"id":"call_b","function":{"name":"write","arguments":"{\"b\":"}}
    ]},"finish_reason":null}]}),
        )
        .unwrap();
    assert!(
        events.iter().any(|event| matches!(event, ModelEvent::FunctionCallDelta { identity, .. } if identity.call_id.as_deref() == Some("call_a")))
    );
    assert!(
        events.iter().any(|event| matches!(event, ModelEvent::FunctionCallDelta { identity, .. } if identity.call_id.as_deref() == Some("call_b")))
    );
    assert!(stream.finish().is_err());
}

#[test]
fn tool_arguments_are_buffered_until_the_call_id_arrives() {
    let mut stream = ChatStream::new("fixture".into());
    let first = stream
        .push(json!({
            "id":"chatcmpl-1",
            "choices":[{"index":0,"delta":{"tool_calls":[{
                "index":0,"function":{"name":"read","arguments":"{\"path\":"}
            }]},"finish_reason":null}]
        }))
        .unwrap();
    assert!(
        first
            .iter()
            .all(|event| !matches!(event, ModelEvent::FunctionCallDelta { .. }))
    );
    let second = stream
        .push(json!({
            "id":"chatcmpl-1",
            "choices":[{"index":0,"delta":{"tool_calls":[{
                "index":0,"id":"call_1","function":{"arguments":"\"a\"}"}
            }]},"finish_reason":null}]
        }))
        .unwrap();
    assert!(second.iter().any(|event| matches!(
        event,
        ModelEvent::FunctionCallDelta { arguments_delta, identity, .. }
            if arguments_delta == "{\"path\":\"a\"}"
                && identity.call_id.as_deref() == Some("call_1")
    )));
}

#[test]
fn streaming_and_non_streaming_fixtures_are_semantically_equivalent() {
    let complete = ChatCompletionsAdapter.decode_response(json!({
        "choices":[{"index":0,"message":{"content":"done","refusal":"no","reasoning_content":"why","tool_calls":[
            {"id":"call_a","function":{"name":"read","arguments":"{}"}},
            {"id":"call_b","function":{"name":"write","arguments":"{\"x\":1}"}}
        ]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":4}
    }), &target()).unwrap();
    let mut stream = ChatStream::new("fixture".into());
    let mut events = Vec::new();
    for chunk in [
        json!({"id":"chat_1","choices":[{"index":0,"delta":{"reasoning_content":"why"},"finish_reason":null}]}),
        json!({"id":"chat_1","choices":[{"index":0,"delta":{"content":"done","refusal":"no"},"finish_reason":null}]}),
        json!({"id":"chat_1","choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"id":"call_a","function":{"name":"read","arguments":"{}"}},
            {"index":1,"id":"call_b","function":{"name":"write","arguments":"{\"x\":1}"}}
        ]},"finish_reason":null}]}),
        json!({"id":"chat_1","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
        json!({"id":"chat_1","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":4}}),
    ] {
        events.extend(stream.push(chunk).unwrap());
    }
    events.extend(stream.finish().unwrap());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ModelEvent::TextDelta { delta, .. } if delta == "done"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ModelEvent::RefusalDelta { delta, .. } if delta == "no"))
    );
    assert!(
        events.iter().any(
            |event| matches!(event, ModelEvent::ReasoningDelta { delta, .. } if delta == "why")
        )
    );
    assert!(events.iter().any(|event| matches!(event, ModelEvent::FunctionCallDelta { identity, .. } if identity.call_id.as_deref() == Some("call_a"))));
    assert!(events.iter().any(|event| matches!(event, ModelEvent::FunctionCallDelta { identity, .. } if identity.call_id.as_deref() == Some("call_b"))));
    assert!(events.iter().any(|event| matches!(event, ModelEvent::Usage(usage) if usage == complete.usage.as_ref().unwrap())));
    assert!(events.iter().any(|event| matches!(event,
        ModelEvent::OutputMetadata(metadata) if metadata == &complete.output_metadata
    )));
    assert!(
        events.iter().any(
            |event| matches!(event, ModelEvent::Completed(status) if status == &complete.status)
        )
    );
}
