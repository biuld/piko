use piko_protocol::messages::{ContentBlock, MessageContent};

pub(super) fn build_genai_messages(
    run_prompt: &piko_protocol::SemanticRunPrompt,
    transcript: &[piko_protocol::messages::Message],
) -> Vec<genai::chat::ChatMessage> {
    use piko_protocol::InstructionAuthority;
    let mut messages = Vec::with_capacity(transcript.len() + 3);
    let is_high_authority = |authority| {
        matches!(
            authority,
            InstructionAuthority::Platform
                | InstructionAuthority::Operator
                | InstructionAuthority::Agent
        )
    };
    let system = run_prompt
        .blocks
        .iter()
        .filter(|block| is_high_authority(block.authority))
        .map(render_prompt_block)
        .collect::<Vec<_>>()
        .join("\n\n");
    if !system.is_empty() {
        messages.push(genai::chat::ChatMessage::system(system));
    }

    let stable_context = run_prompt
        .blocks
        .iter()
        .filter(|block| {
            !is_high_authority(block.authority)
                && !matches!(
                    block.cache_scope,
                    piko_protocol::CacheScope::RunDynamic | piko_protocol::CacheScope::NoCache
                )
        })
        .map(render_prompt_block)
        .collect::<Vec<_>>()
        .join("\n\n");
    if !stable_context.is_empty() {
        messages.push(genai::chat::ChatMessage::user(format!(
            "[piko stable run context; preserve each block's labeled authority and trust]\n{stable_context}"
        )));
    }
    if run_prompt.cache_plan.policy != piko_protocol::PromptCachePolicy::Disabled
        && let Some(last) = messages.last_mut()
    {
        *last = last
            .clone()
            .with_options(genai::chat::CacheControl::Ephemeral);
    }

    let dynamic_context = run_prompt
        .blocks
        .iter()
        .filter(|block| {
            !is_high_authority(block.authority)
                && matches!(
                    block.cache_scope,
                    piko_protocol::CacheScope::RunDynamic | piko_protocol::CacheScope::NoCache
                )
        })
        .map(render_prompt_block)
        .collect::<Vec<_>>()
        .join("\n\n");
    if !dynamic_context.is_empty() {
        messages.push(genai::chat::ChatMessage::user(format!(
            "[piko dynamic run context; preserve each block's labeled authority and trust]\n{dynamic_context}"
        )));
    }

    // Fuse each durable Assistant with immediately following ToolCall items
    // into one provider-facing ChatMessage (model-turn shape). Domain
    // transcripts stay itemized; only this outbound projection groups them.
    let mut idx = 0;
    while idx < transcript.len() {
        match &transcript[idx] {
            piko_protocol::messages::Message::Assistant { content, .. } => {
                let mut parts = assistant_content_parts(content);
                idx += 1;
                while let Some(piko_protocol::messages::Message::ToolCall {
                    id,
                    name,
                    arguments,
                    ..
                }) = transcript.get(idx)
                {
                    parts.push(tool_call_part(id, name, arguments));
                    idx += 1;
                }
                messages.push(genai::chat::ChatMessage::assistant(parts));
            }
            message => {
                messages.push(map_transcript_message(message));
                if let piko_protocol::messages::Message::ToolResult {
                    tool_call_id,
                    content,
                    ..
                } = message
                {
                    push_tool_result_images(&mut messages, tool_call_id, content);
                }
                idx += 1;
            }
        }
    }
    messages
}

fn map_transcript_message(message: &piko_protocol::messages::Message) -> genai::chat::ChatMessage {
    match message {
        piko_protocol::messages::Message::Context {
            content,
            trust,
            source,
            ..
        } => genai::chat::ChatMessage::user(format!(
            "[piko data-only context; authority=None; trust={trust:?}; source={}:{}]\n{}",
            source.kind,
            source.locator,
            message_content_text(content)
        )),
        piko_protocol::messages::Message::User { content, .. } => {
            genai::chat::ChatMessage::user(content_parts(content))
        }
        piko_protocol::messages::Message::Assistant { content, .. } => {
            build_assistant_message(content)
        }
        piko_protocol::messages::Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => {
            // Orphan tool call (no preceding Assistant in the run of items).
            genai::chat::ChatMessage::assistant(vec![tool_call_part(id, name, arguments)])
        }
        piko_protocol::messages::Message::ToolResult {
            tool_call_id,
            content,
            ..
        } => {
            let content = genai::chat::MessageContent::from_parts(vec![
                genai::chat::ContentPart::ToolResponse(genai::chat::ToolResponse::new(
                    tool_call_id.clone(),
                    extract_blocks(content),
                )),
            ]);
            genai::chat::ChatMessage::new(genai::chat::ChatRole::Tool, content)
        }
    }
}

fn push_tool_result_images(
    messages: &mut Vec<genai::chat::ChatMessage>,
    tool_call_id: &str,
    content: &[ContentBlock],
) {
    let mut images = vec![genai::chat::ContentPart::Text(format!(
        "[piko data-only image content from tool result {tool_call_id}; authority=None; trust=Untrusted]"
    ))];
    images.extend(content.iter().filter_map(|block| match block {
        ContentBlock::Image { data, mime_type } => Some(
            genai::chat::ContentPart::from_binary_base64(mime_type, data.clone(), None),
        ),
        _ => None,
    }));
    if images.len() > 1 {
        messages.push(genai::chat::ChatMessage::user(images));
    }
}

fn tool_call_part(id: &str, name: &str, arguments: &serde_json::Value) -> genai::chat::ContentPart {
    genai::chat::ContentPart::ToolCall(genai::chat::ToolCall {
        call_id: id.to_string(),
        fn_name: name.to_string(),
        fn_arguments: arguments.clone(),
        thought_signatures: None,
    })
}

fn message_content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => extract_blocks(blocks),
    }
}

fn assistant_content_parts(content: &[ContentBlock]) -> Vec<genai::chat::ContentPart> {
    let mut parts = Vec::with_capacity(content.len() * 2);
    for block in content {
        match block {
            ContentBlock::Text { text } => {
                parts.push(genai::chat::ContentPart::Text(text.clone()));
            }
            ContentBlock::Thinking {
                thinking,
                thinking_signature,
            } => {
                parts.push(genai::chat::ContentPart::ReasoningContent(thinking.clone()));
                if let Some(signature) = thinking_signature {
                    parts.push(genai::chat::ContentPart::ThoughtSignature(
                        signature.clone(),
                    ));
                }
            }
            ContentBlock::Image { data, mime_type } => parts.push(
                genai::chat::ContentPart::from_binary_base64(mime_type, data.clone(), None),
            ),
        }
    }
    parts
}

fn build_assistant_message(content: &[ContentBlock]) -> genai::chat::ChatMessage {
    genai::chat::ChatMessage::assistant(assistant_content_parts(content))
}

fn content_parts(content: &MessageContent) -> genai::chat::MessageContent {
    match content {
        MessageContent::String(text) => text.clone().into(),
        MessageContent::Blocks(blocks) => genai::chat::MessageContent::from_parts(
            blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => {
                        Some(genai::chat::ContentPart::Text(text.clone()))
                    }
                    ContentBlock::Image { data, mime_type } => Some(
                        genai::chat::ContentPart::from_binary_base64(mime_type, data.clone(), None),
                    ),
                    ContentBlock::Thinking { .. } => None,
                })
                .collect::<Vec<_>>(),
        ),
    }
}

fn render_prompt_block(block: &piko_protocol::PromptBlock) -> String {
    let metadata = serde_json::json!({
        "id": block.id,
        "authority": block.authority,
        "trust": block.trust,
        "source": block.source,
    });
    format!("[piko prompt block {metadata}]\n{}", block.content)
}

pub(super) fn stateless_system_block(content: String) -> piko_protocol::PromptBlock {
    piko_protocol::PromptBlock {
        id: "stateless.system".into(),
        kind: piko_protocol::PromptBlockKind::Instruction,
        authority: piko_protocol::InstructionAuthority::Platform,
        trust: piko_protocol::ContentTrust::Trusted,
        source: piko_protocol::PromptSource::new("stateless-call", "llm-call"),
        content_digest: String::new(),
        content,
        cache_scope: piko_protocol::CacheScope::NoCache,
    }
}

fn extract_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
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
        let messages =
            build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
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
        let messages =
            build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);

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
    fn fuses_each_model_step_separately() {
        let transcript = vec![
            assistant_with_thinking("first", ""),
            tool_call("c1", "a"),
            tool_result("c1", "ok"),
            assistant_with_thinking("second", "done"),
            tool_call("c2", "b"),
            tool_result("c2", "ok"),
        ];
        let messages =
            build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
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
        let messages =
            build_genai_messages(&piko_protocol::SemanticRunPrompt::default(), &transcript);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, genai::chat::ChatRole::Assistant);
        assert_eq!(messages[0].content.tool_calls().len(), 1);
    }
}
