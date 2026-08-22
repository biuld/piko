use super::*;

#[test]
fn standard_responses_encodes_catalog_defined_non_search_tool() {
    let mut config = ModelTargetConfig::new(
        "fixture/gpt@responses",
        "responses",
        piko_protocol::model::ProviderAuthMethod::ApiKey,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::PreviousResponseId,
            variant: ResponsesVariant::Standard,
        },
    );
    config.base_url = Some("https://example.test/v1".into());
    config.upstream_tool_catalog.insert(
        crate::capabilities::UpstreamToolKind::new("future_media").unwrap(),
        crate::modeling::UpstreamToolSupport {
            kind: crate::capabilities::UpstreamToolKind::new("future_media").unwrap(),
            name: "future_media".into(),
            approval: crate::tools::UpstreamApprovalPolicy::OnRequest,
            wire_definition: serde_json::json!({
                "type":"future_media",
                "quality":"medium"
            }),
            wire_choice: serde_json::json!({"type":"future_media"}),
            activity_types: vec!["future_media_call".into()],
        },
    );
    config.capabilities = Some(crate::target::ModelCapabilities {
        upstream_dispatch: true,
        ..Default::default()
    });
    let target = ModelTarget::resolve("fixture/gpt", "gpt", &config, None).unwrap();
    let mut request = crate::protocols::tests_support::semantic_request();
    request.options.allow_upstream_tools = true;
    request.tools.push(crate::tools::InferenceTool::Upstream(
        crate::tools::UpstreamToolDefinition {
            name: "future_media".into(),
            kind: crate::capabilities::UpstreamToolKind::new("future_media").unwrap(),
            resources: Vec::new(),
            approval: crate::tools::UpstreamApprovalPolicy::OnRequest,
        },
    ));
    target.resolve_upstream_tools(&mut request).unwrap();
    let plan = plan(&target, &request.conversation).unwrap();
    let body = ResponsesAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert!(
        body["tools"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!({"type":"future_media", "quality":"medium"}))
    );
}

#[test]
fn stream_accepts_only_catalog_owned_upstream_lifecycle_notifications() {
    let mut stream = ResponsesStream::new(
        target_with_upstream_search(),
        crate::protocols::tests_support::semantic_request(),
    );
    stream
        .push(json!({"type":"response.created","response":{"id":"resp_search"}}))
        .unwrap();

    for event_type in [
        "response.web_search_call.in_progress",
        "response.web_search_call.searching",
        "response.web_search_call.completed",
    ] {
        assert!(
            stream
                .push(json!({"type":event_type,"output_index":0,"item_id":"ws_1"}))
                .unwrap()
                .is_empty()
        );
    }

    let error = stream
        .push(json!({"type":"response.unknown_call.in_progress"}))
        .unwrap_err();
    assert!(
        error
            .message
            .contains("unsupported required stream event type")
    );
}
