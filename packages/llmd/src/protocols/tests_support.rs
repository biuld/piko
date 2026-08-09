use crate::gateway::{
    Conversation, InferenceOptions, InferenceRequest, InvocationContext, ModelRef,
};
use piko_protocol::tools::{ToolDef, ToolExecutorRef};
use piko_protocol::{
    CacheScope, ContentBlock, ContentTrust, InstructionAuthority, Message, MessageContent,
    PromptBlock, PromptBlockKind, PromptSource, SemanticRunPrompt,
};
use serde_json::json;

pub(crate) fn semantic_request() -> InferenceRequest {
    let transcript = vec![
        Message::User {
            content: MessageContent::String("question".into()),
            timestamp: None,
        },
        Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "calling".into(),
            }],
            checkpoint: None,
            provider: "fixture".into(),
            model: "gpt".into(),
            usage: None,
            stop_reason: Some("tool_calls".into()),
            error_message: None,
            timestamp: None,
        },
        Message::ToolCall {
            id: "call_a".into(),
            name: "read".into(),
            arguments: json!({"path":"a"}),
            model: None,
            provider: None,
            timestamp: None,
        },
        Message::ToolCall {
            id: "call_b".into(),
            name: "read".into(),
            arguments: json!({"path":"b"}),
            model: None,
            provider: None,
            timestamp: None,
        },
        Message::ToolResult {
            tool_call_id: "call_a".into(),
            tool_name: Some("read".into()),
            content: vec![ContentBlock::Text { text: "A".into() }],
            details: None,
            is_error: Some(false),
            timestamp: None,
        },
        Message::ToolResult {
            tool_call_id: "call_b".into(),
            tool_name: Some("read".into()),
            content: vec![ContentBlock::Text { text: "B".into() }],
            details: None,
            is_error: Some(false),
            timestamp: None,
        },
    ];
    let mut run_prompt = SemanticRunPrompt::default();
    run_prompt.blocks.push(PromptBlock {
        id: "system".into(),
        kind: PromptBlockKind::Instruction,
        authority: InstructionAuthority::Platform,
        trust: ContentTrust::Trusted,
        source: PromptSource::new("fixture", "system"),
        content: "be exact".into(),
        content_digest: String::new(),
        cache_scope: CacheScope::NoCache,
    });
    InferenceRequest {
        model: ModelRef::new("fixture", "gpt"),
        conversation: Conversation::from_messages(run_prompt, transcript),
        tools: vec![
            ToolDef {
                name: "read".into(),
                version: "1".into(),
                provenance: PromptSource::new("fixture", "read"),
                description: "read a path".into(),
                input_schema: json!({"type":"object"}),
                executor: ToolExecutorRef {
                    kind: "native".into(),
                    target: "read".into(),
                    extra: None,
                },
                execution_mode: None,
                exposure: None,
                capabilities: None,
                approval: None,
                metadata: None,
            }
            .into(),
        ],
        options: InferenceOptions {
            reasoning_effort: Some(piko_protocol::model::ThinkingLevel::High),
            ..Default::default()
        },
        context: InvocationContext {
            session_id: "session".into(),
            agent_instance_id: "root".into(),
            run_id: "run".into(),
            step_id: "step".into(),
        },
    }
}
