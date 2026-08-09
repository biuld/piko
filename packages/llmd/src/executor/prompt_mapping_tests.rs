use super::*;

fn block(
    id: &str,
    authority: piko_protocol::InstructionAuthority,
    scope: piko_protocol::CacheScope,
    content: &str,
) -> piko_protocol::PromptBlock {
    piko_protocol::PromptBlock {
        id: id.into(),
        kind: piko_protocol::PromptBlockKind::Instruction,
        authority,
        trust: piko_protocol::ContentTrust::WorkspaceControlled,
        source: piko_protocol::PromptSource::new("test", id),
        content: content.into(),
        content_digest: format!("digest-{id}"),
        cache_scope: scope,
    }
}

#[test]
fn project_content_is_not_promoted_to_system_authority() {
    let prompt = piko_protocol::SemanticRunPrompt {
        blocks: vec![
            block(
                "platform",
                piko_protocol::InstructionAuthority::Platform,
                piko_protocol::CacheScope::GlobalStable,
                "platform policy",
            ),
            block(
                "project",
                piko_protocol::InstructionAuthority::Project,
                piko_protocol::CacheScope::ResourceSnapshot,
                "project policy",
            ),
        ],
        cache_plan: piko_protocol::PromptCachePlan {
            policy: piko_protocol::PromptCachePolicy::ProviderDefault,
            ..Default::default()
        },
        ..Default::default()
    };

    let messages = build_genai_messages(&prompt, &[]);
    assert_eq!(messages[0].role, genai::chat::ChatRole::System);
    assert_eq!(messages[1].role, genai::chat::ChatRole::User);
    let system = messages[0].content.clone().into_texts().join("\n");
    let context = messages[1].content.clone().into_texts().join("\n");
    assert!(system.contains("platform policy"));
    assert!(!system.contains("project policy"));
    assert!(context.contains("project policy"));
    assert!(messages[1].options.is_some());
}

#[test]
fn dynamic_suffix_comes_after_the_cache_breakpoint() {
    let prompt = piko_protocol::SemanticRunPrompt {
        blocks: vec![
            block(
                "project",
                piko_protocol::InstructionAuthority::Project,
                piko_protocol::CacheScope::ResourceSnapshot,
                "stable",
            ),
            block(
                "environment",
                piko_protocol::InstructionAuthority::None,
                piko_protocol::CacheScope::RunDynamic,
                "today",
            ),
        ],
        cache_plan: piko_protocol::PromptCachePlan {
            policy: piko_protocol::PromptCachePolicy::Ephemeral,
            ..Default::default()
        },
        ..Default::default()
    };

    let messages = build_genai_messages(&prompt, &[]);
    assert_eq!(messages.len(), 2);
    assert!(messages[0].options.is_some());
    assert!(messages[1].options.is_none());
}

#[test]
fn assistant_reasoning_signature_and_image_are_preserved() {
    let message = build_assistant_message(&[
        ContentBlock::Thinking {
            thinking: "reason".into(),
            thinking_signature: Some("signature".into()),
        },
        ContentBlock::Image {
            data: "aGVsbG8=".into(),
            mime_type: "image/png".into(),
        },
    ]);
    let parts = message.content.parts();
    assert!(parts.iter().any(|part| matches!(part, genai::chat::ContentPart::ReasoningContent(value) if value == "reason")));
    assert!(parts.iter().any(|part| matches!(part, genai::chat::ContentPart::ThoughtSignature(value) if value == "signature")));
    assert!(
        parts
            .iter()
            .any(|part| matches!(part, genai::chat::ContentPart::Binary(_)))
    );
}

#[test]
fn tool_result_images_are_preserved_as_untrusted_data_context() {
    let transcript = vec![piko_protocol::Message::ToolResult {
        tool_call_id: "call-1".into(),
        tool_name: Some("view".into()),
        content: vec![ContentBlock::Image {
            data: "aGVsbG8=".into(),
            mime_type: "image/png".into(),
        }],
        details: None,
        is_error: None,
        timestamp: None,
    }];
    let messages = build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, genai::chat::ChatRole::Tool);
    assert_eq!(messages[1].role, genai::chat::ChatRole::User);
    assert!(
        messages[1]
            .content
            .parts()
            .iter()
            .any(|part| matches!(part, genai::chat::ContentPart::Binary(_)))
    );
}

fn assistant_with_thinking(thinking: &str, text: &str) -> piko_protocol::Message {
    piko_protocol::Message::Assistant {
        content: vec![
            ContentBlock::Thinking {
                thinking: thinking.into(),
                thinking_signature: None,
            },
            ContentBlock::Text { text: text.into() },
        ],
        api: "openai-completions".into(),
        provider: "deepseek".into(),
        model: "deepseek-v4-flash".into(),
        usage: None,
        stop_reason: Some("tool_use".into()),
        error_message: None,
        timestamp: None,
    }
}

fn tool_call(id: &str, name: &str) -> piko_protocol::Message {
    piko_protocol::Message::ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: serde_json::json!({}),
        model: Some("deepseek-v4-flash".into()),
        provider: Some("deepseek".into()),
        timestamp: None,
    }
}

fn tool_result(id: &str, body: &str) -> piko_protocol::Message {
    piko_protocol::Message::ToolResult {
        tool_call_id: id.into(),
        tool_name: None,
        content: vec![ContentBlock::Text { text: body.into() }],
        details: None,
        is_error: None,
        timestamp: None,
    }
}

#[test]
fn fuses_assistant_thinking_with_following_tool_calls_into_one_message() {
    let transcript = vec![
        piko_protocol::Message::User {
            content: MessageContent::String("spawn an agent".into()),
            timestamp: None,
        },
        assistant_with_thinking("list specs and agents", ""),
        tool_call("call-1", "list_agent_specs"),
        tool_call("call-2", "list_agents"),
        tool_result("call-1", r#"{"specs":[]}"#),
        tool_result("call-2", r#"{"agents":[]}"#),
    ];
    let messages = build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, genai::chat::ChatRole::User);
    assert_eq!(messages[1].role, genai::chat::ChatRole::Assistant);
    assert_eq!(messages[2].role, genai::chat::ChatRole::Tool);
    assert_eq!(messages[3].role, genai::chat::ChatRole::Tool);

    let parts = messages[1].content.parts();
    assert!(parts.iter().any(|part| matches!(
        part,
        genai::chat::ContentPart::ReasoningContent(value)
            if value == "list specs and agents"
    )));
    let tool_calls: Vec<_> = parts
        .iter()
        .filter_map(|part| match part {
            genai::chat::ContentPart::ToolCall(tc) => Some(tc.fn_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_calls, vec!["list_agent_specs", "list_agents"]);
}

#[test]
fn normalizes_legacy_interleaved_tool_exchange_into_one_assistant_message() {
    let transcript = vec![
        assistant_with_thinking("inspect workspace", ""),
        tool_call("call-1", "diff_stat"),
        tool_result("call-1", "changed"),
        tool_call("call-2", "git_log"),
        tool_result("call-2", "commit"),
    ];
    let messages = build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].role, genai::chat::ChatRole::Assistant);
    assert_eq!(messages[1].role, genai::chat::ChatRole::Tool);
    assert_eq!(messages[2].role, genai::chat::ChatRole::Tool);
    assert!(messages[0].content.parts().iter().any(|part| matches!(
        part,
        genai::chat::ContentPart::ReasoningContent(value)
            if value == "inspect workspace"
    )));
    let tool_calls: Vec<_> = messages[0]
        .content
        .tool_calls()
        .iter()
        .map(|call| call.fn_name.as_str())
        .collect();
    assert_eq!(tool_calls, ["diff_stat", "git_log"]);
}

#[test]
fn fuses_each_model_step_separately() {
    let transcript = vec![
        assistant_with_thinking("first", ""),
        tool_call("c1", "a"),
        tool_result("c1", "ok"),
        assistant_with_thinking("second", "done"),
        tool_call("c2", "b"),
        tool_result("c2", "ok"),
    ];
    let messages = build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, genai::chat::ChatRole::Assistant);
    assert_eq!(messages[1].role, genai::chat::ChatRole::Tool);
    assert_eq!(messages[2].role, genai::chat::ChatRole::Assistant);
    assert_eq!(messages[3].role, genai::chat::ChatRole::Tool);
    assert_eq!(messages[0].content.tool_calls().len(), 1);
    assert_eq!(messages[2].content.tool_calls().len(), 1);
}

#[test]
fn orphan_tool_call_still_maps_when_missing_assistant() {
    let transcript = vec![tool_call("orphan", "solo")];
    let messages = build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, genai::chat::ChatRole::Assistant);
    assert_eq!(messages[0].content.tool_calls().len(), 1);
}
