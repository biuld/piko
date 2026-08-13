use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::PromptDebugMap;

pub(super) struct HostPromptAssemblyPort {
    pub(super) snapshots: Arc<Mutex<PromptDebugMap>>,
}

#[async_trait]
impl piko_orchd_api::PromptAssemblyPort for HostPromptAssemblyPort {
    async fn assemble_prompt(
        &self,
        request: piko_protocol::PromptAssemblyRequest,
    ) -> Result<piko_protocol::SemanticRunPrompt, piko_orchd_api::AgentApiError> {
        let resource_count = usize::from(request.resources.world_state.is_some())
            + request.resources.user_mentions.len();
        let span = tracing::info_span!(
            "piko.prompt.assemble",
            session_id = %request.session_id,
            run_id = %request.run_id,
            agent_instance_id = %request.agent_instance_id,
            agent_spec_id = %request.agent_spec.id,
            assembly_version = piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            input_blocks = request.resources.blocks.len(),
            resources = resource_count,
            tools = request.tool_catalog.tools.len(),
            tool_catalog_digest = %request.tool_catalog.digest,
            output_blocks = tracing::field::Empty,
            source_digest = tracing::field::Empty,
            semantic_prefix_digest = tracing::field::Empty,
            cache_segments = tracing::field::Empty,
            "piko.prompt.blocks" = tracing::field::Empty,
            "piko.prompt.tool_sources" = tracing::field::Empty,
            "piko.prompt.metadata_dropped" = tracing::field::Empty,
        );
        span.in_scope(|| {
            let run_prompt = crate::domain::prompts::assemble_agent_run_prompt(&request);
            record_assembly_provenance(&span, &request, &run_prompt);
            crate::telemetry::handle().begin_prompt_run(
                &request.session_id,
                &request.agent_instance_id,
                &request.run_id,
            );
            let mut resource_messages = Vec::new();
            if let Some(world_state) = request.resources.world_state.clone() {
                resource_messages.push(world_state);
            }
            resource_messages.extend(request.resources.user_mentions.clone());
            let snapshot = piko_protocol::PromptDebugSnapshot {
                session_id: request.session_id.clone(),
                agent_instance_id: request.agent_instance_id.clone(),
                run_id: request.run_id.clone(),
                run_prompt: run_prompt.clone(),
                resource_messages,
                tool_catalog: request.tool_catalog.clone(),
                model_inputs: Vec::new(),
            };
            self.snapshots
                .lock()
                .unwrap()
                .insert((request.session_id, request.agent_instance_id), snapshot);
            Ok(run_prompt)
        })
    }
}

fn record_assembly_provenance(
    span: &tracing::Span,
    request: &piko_protocol::PromptAssemblyRequest,
    prompt: &piko_protocol::SemanticRunPrompt,
) {
    let mut metadata_dropped = false;
    let blocks = prompt
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            serde_json::json!({
                "index": index,
                "id": block.id,
                "kind": block.kind,
                "authority": block.authority,
                "trust": block.trust,
                "source": block.source,
                "cacheScope": block.cache_scope,
                "contentDigest": block.content_digest,
                "contentChars": block.content.chars().count(),
            })
        })
        .collect::<Vec<_>>();
    if let Ok(value) = serde_json::to_string(&blocks)
        && value.len() <= 64 * 1024
    {
        span.record("piko.prompt.blocks", value.as_str());
    } else {
        metadata_dropped = true;
    }
    if let Ok(value) = serde_json::to_string(&request.tool_catalog.sources)
        && value.len() <= 64 * 1024
    {
        span.record("piko.prompt.tool_sources", value.as_str());
    } else {
        metadata_dropped = true;
    }
    if metadata_dropped {
        span.record("piko.prompt.metadata_dropped", true);
    }
    span.record("output_blocks", prompt.blocks.len());
    span.record("source_digest", prompt.source_digest.as_str());
    span.record(
        "semantic_prefix_digest",
        prompt.cache_plan.semantic_prefix_digest.as_str(),
    );
    span.record("cache_segments", prompt.cache_plan.prefix_segments.len());
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use piko_orchd_api::PromptAssemblyPort;
    use piko_protocol::{
        AgentSpec, ContentTrust, Message, MessageContent, PromptAssemblyRequest,
        PromptResourceSnapshot, PromptSource, ResolvedToolCatalog,
    };

    use super::HostPromptAssemblyPort;

    fn request(session_id: &str, agent_instance_id: &str, marker: &str) -> PromptAssemblyRequest {
        PromptAssemblyRequest {
            session_id: session_id.into(),
            agent_instance_id: agent_instance_id.into(),
            run_id: format!("run-{marker}"),
            agent_spec: AgentSpec {
                id: "main".into(),
                version: "1".into(),
                provenance: PromptSource::new("test", "main"),
                name: "Main".into(),
                role: "root".into(),
                description: None,
                base_instructions: marker.into(),
                model: None,
                thinking_level: None,
                tool_set_ids: Vec::new(),
                active_tool_names: None,
            },
            resources: PromptResourceSnapshot {
                world_state: Some(Message::Context {
                    content: MessageContent::String("world".into()),
                    trust: ContentTrust::Trusted,
                    source: PromptSource::new("runtime", "world"),
                    timestamp: None,
                }),
                user_mentions: vec![Message::Context {
                    content: MessageContent::String("mention".into()),
                    trust: ContentTrust::WorkspaceControlled,
                    source: PromptSource::new("mention", "file"),
                    timestamp: None,
                }],
                ..Default::default()
            },
            tool_catalog: ResolvedToolCatalog::new(Vec::new(), format!("tools-{marker}")),
        }
    }

    #[tokio::test]
    async fn captures_and_replaces_successful_assembly_per_agent() {
        let snapshots = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
        let port = HostPromptAssemblyPort {
            snapshots: snapshots.clone(),
        };

        let first = port
            .assemble_prompt(request("s1", "a1", "first"))
            .await
            .unwrap();
        port.assemble_prompt(request("s1", "a2", "other"))
            .await
            .unwrap();
        let second = port
            .assemble_prompt(request("s1", "a1", "second"))
            .await
            .unwrap();

        let snapshots = snapshots.lock().unwrap();
        assert_eq!(snapshots.len(), 2);
        let captured = snapshots.get(&("s1".into(), "a1".into())).unwrap();
        assert_eq!(captured.run_prompt, second);
        assert_eq!(captured.run_id, "run-second");
        assert_ne!(captured.run_prompt, first);
        assert_eq!(captured.resource_messages.len(), 2);
        assert!(matches!(
            &captured.resource_messages[0],
            Message::Context {
                content: MessageContent::String(content),
                ..
            } if content == "world"
        ));
        assert!(matches!(
            &captured.resource_messages[1],
            Message::Context {
                content: MessageContent::String(content),
                ..
            } if content == "mention"
        ));
        assert_eq!(captured.tool_catalog.digest, "tools-second");
        assert_eq!(
            snapshots
                .get(&("s1".into(), "a2".into()))
                .unwrap()
                .tool_catalog
                .digest,
            "tools-other"
        );
    }
}
