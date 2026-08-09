use std::collections::HashMap;

use piko_protocol::model::ProviderAuthMethod;

use super::*;
use crate::modeling::ProtocolProfile;

fn config(protocol: ProtocolProfile) -> ModelTargetConfig {
    let mut config = ModelTargetConfig::new(
        "custom/model@platform",
        "platform",
        ProviderAuthMethod::ApiKey,
        protocol,
    );
    config.base_url = Some("https://example.test/v1".into());
    config
}

#[test]
fn protocol_alone_selects_operation_path() {
    let responses = ModelTarget::resolve(
        "custom",
        "same-model",
        &config(ProtocolProfile::Responses {
            continuation: Default::default(),
        }),
        None,
    )
    .unwrap();
    let chat = ModelTarget::resolve(
        "custom",
        "same-model",
        &config(ProtocolProfile::ChatCompletions),
        None,
    )
    .unwrap();
    assert_eq!(
        responses.endpoint.as_str(),
        "https://example.test/v1/responses"
    );
    assert_eq!(
        chat.endpoint.as_str(),
        "https://example.test/v1/chat/completions"
    );
}

#[test]
fn explicit_endpoint_is_not_rewritten() {
    let mut config = config(ProtocolProfile::Responses {
        continuation: Default::default(),
    });
    config.endpoint = Some("https://example.test/custom/inference".into());
    let target = ModelTarget::resolve("custom/model", "gpt", &config, None).unwrap();
    assert_eq!(
        target.endpoint.as_str(),
        "https://example.test/custom/inference"
    );
}

#[test]
fn custom_headers_cannot_override_auth() {
    let mut config = config(ProtocolProfile::Responses {
        continuation: Default::default(),
    });
    config.headers = Some(HashMap::from([("Authorization".into(), "stolen".into())]));
    assert!(ModelTarget::resolve("custom", "gpt", &config, None).is_err());
}

#[test]
fn capabilities_fail_before_dispatch() {
    let mut unsupported = config(ProtocolProfile::Responses {
        continuation: Default::default(),
    });
    unsupported.capabilities = Some(ModelCapabilities {
        tools: false,
        ..Default::default()
    });
    let target = ModelTarget::resolve("custom", "gpt", &unsupported, None).unwrap();
    let mut request = crate::protocols::tests_support::semantic_request();
    request.model.provider = "custom".into();
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );
}

#[test]
fn requested_semantics_are_rejected_instead_of_silently_dropped() {
    let mut config = config(ProtocolProfile::ChatCompletions);
    config.capabilities = Some(ModelCapabilities {
        reasoning_efforts: [piko_protocol::model::ThinkingLevel::Low]
            .into_iter()
            .collect(),
        parallel_tools: false,
        structured_json_schema: false,
        streaming_delivery: false,
        assembled_delivery: true,
        ..Default::default()
    });
    let target = ModelTarget::resolve("custom", "gpt", &config, None).unwrap();
    let mut request = crate::protocols::tests_support::semantic_request();
    request.options.reasoning_effort = Some(piko_protocol::model::ThinkingLevel::High);
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );

    request.options.reasoning_effort = None;
    request.options.parallel_tools = Some(true);
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );

    request.options.parallel_tools = None;
    request.options.structured_output = Some(crate::gateway::StructuredOutputIntent {
        schema: serde_json::json!({"type":"object"}),
        strict: false,
    });
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );

    request.options.structured_output = None;
    request.options.delivery = crate::gateway::DeliveryMode::Streaming;
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );
}

#[test]
fn upstream_catalog_support_is_not_execution_authorization() {
    use crate::capabilities::UpstreamToolKind;
    use crate::tools::{InferenceTool, UpstreamApprovalPolicy, UpstreamToolDefinition};

    let mut config = config(ProtocolProfile::Responses {
        continuation: Default::default(),
    });
    config.capabilities = Some(ModelCapabilities {
        upstream_tools: [UpstreamToolKind::Search].into_iter().collect(),
        ..Default::default()
    });
    let target = ModelTarget::resolve("custom", "gpt", &config, None).unwrap();
    let mut request = crate::protocols::tests_support::semantic_request();
    request.tools = vec![InferenceTool::Upstream(UpstreamToolDefinition {
        name: "search".into(),
        kind: UpstreamToolKind::Search,
        resources: Vec::new(),
        approval: UpstreamApprovalPolicy::OnRequest,
        authorization: None,
    })];
    let error = target.validate(&request).unwrap_err();
    assert_eq!(error.class, ErrorClass::UnsupportedCapability);
    assert!(error.message.contains("not enabled"));
}

#[test]
fn descriptor_separates_semantic_capabilities_from_target_configuration() {
    let target = ModelTarget::resolve(
        "custom/model",
        "gpt",
        &config(ProtocolProfile::Responses {
            continuation: Default::default(),
        }),
        None,
    )
    .unwrap();
    let descriptor = target.descriptor("custom");
    assert_eq!(
        descriptor.model,
        crate::gateway::ModelRef::new("custom", "gpt")
    );
    assert!(
        descriptor
            .capabilities
            .tools
            .loci
            .contains(&crate::capabilities::ToolExecutionLocus::Caller)
    );
    let serialized = serde_json::to_string(&descriptor).unwrap();
    assert!(!serialized.contains("example.test"));
    assert!(!serialized.contains("responses"));
    assert!(!serialized.contains("previous_response"));
}

#[test]
fn unsupported_input_modality_is_rejected_before_encoding() {
    let mut config = config(ProtocolProfile::ChatCompletions);
    config.capabilities = Some(ModelCapabilities {
        images: false,
        ..Default::default()
    });
    let target = ModelTarget::resolve("custom", "gpt", &config, None).unwrap();
    let mut request = crate::protocols::tests_support::semantic_request();
    request.conversation.items[0].kind = crate::gateway::ConversationItemKind::User {
        content: piko_protocol::MessageContent::Blocks(vec![piko_protocol::ContentBlock::Image {
            data: "aGVsbG8=".into(),
            mime_type: "image/png".into(),
        }]),
    };
    assert_eq!(
        target.validate(&request).unwrap_err().class,
        ErrorClass::UnsupportedCapability
    );
}
