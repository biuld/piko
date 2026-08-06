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
        let run_prompt = crate::domain::prompts::assemble_agent_run_prompt(&request);
        crate::telemetry::handle()
            .clear_model_inputs(&request.session_id, &request.agent_instance_id);
        let mut resource_messages = Vec::new();
        if let Some(world_state) = request.resources.world_state.clone() {
            resource_messages.push(world_state);
        }
        resource_messages.extend(request.resources.user_mentions.clone());
        let snapshot = piko_protocol::PromptDebugSnapshot {
            session_id: request.session_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
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
    }
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
