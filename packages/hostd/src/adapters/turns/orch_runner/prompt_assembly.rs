use async_trait::async_trait;
use piko_orchd_api::TrajectoryCapturePort;

use crate::infra::trajectory::TrajectoryRecorder;
use crate::util::now_ms;

pub(super) struct HostPromptAssemblyPort {
    pub(super) trajectory: Option<TrajectoryRecorder>,
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
            if let Some(recorder) = self.trajectory.clone() {
                let record = piko_protocol::TrajectoryRecord::Assembly(
                    piko_protocol::TrajectoryAssemblyRecord {
                        identity: piko_protocol::TrajectoryIdentity {
                            session_id: request.session_id.clone(),
                            agent_instance_id: request.agent_instance_id.clone(),
                            run_id: request.run_id.clone(),
                            execution_id: None,
                            source_turn_id: None,
                        },
                        assembly_version: run_prompt.assembly_version,
                        prompt_digest: run_prompt.source_digest.clone(),
                        prompt: run_prompt.clone(),
                        tool_catalog: request.tool_catalog.clone(),
                        recorded_at: now_ms(),
                    },
                );
                tokio::spawn(async move {
                    recorder.record(record).await;
                });
            }
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
    use piko_orchd_api::PromptAssemblyPort;
    use piko_protocol::{
        AgentSpec, ContentTrust, Message, MessageContent, PromptAssemblyRequest,
        PromptResourceSnapshot, PromptSource, ResolvedToolCatalog, TRAJECTORY_EVENT_ASSEMBLY,
        TrajectoryAssemblyRecord,
    };

    use crate::infra::storage::SessionStore;

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
        let temp = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 0).unwrap();
        let recorder =
            crate::infra::trajectory::TrajectoryRecorder::new(store.clone(), "s1".into());
        let port = HostPromptAssemblyPort {
            trajectory: Some(recorder),
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

        let assemblies = wait_for_assemblies(&store, 3).await;
        let a1_second = assemblies
            .iter()
            .find(|record| {
                record.identity.agent_instance_id == "a1" && record.identity.run_id == "run-second"
            })
            .expect("second assembly for a1");
        assert_eq!(a1_second.prompt, second);
        assert_ne!(a1_second.prompt, first);
        assert_eq!(a1_second.tool_catalog.digest, "tools-second");
        assert!(
            assemblies
                .iter()
                .any(|record| record.identity.run_id == "run-first")
        );
        assert!(assemblies.iter().any(|record| {
            record.identity.agent_instance_id == "a2" && record.tool_catalog.digest == "tools-other"
        }));
    }

    async fn wait_for_assemblies(
        store: &SessionStore,
        expected: usize,
    ) -> Vec<TrajectoryAssemblyRecord> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let events = store.raw_journal_events().unwrap_or_default();
            let assemblies = events
                .iter()
                .filter(|event| event.event.event_type == TRAJECTORY_EVENT_ASSEMBLY)
                .filter_map(|event| {
                    serde_json::from_value::<TrajectoryAssemblyRecord>(event.event.payload.clone())
                        .ok()
                })
                .collect::<Vec<_>>();
            if assemblies.len() >= expected {
                return assemblies;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "trajectory assembly records not appended"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
