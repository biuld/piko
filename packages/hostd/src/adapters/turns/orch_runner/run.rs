use std::sync::Arc;

use orchd_api::{AgentRuntimeApi, SessionSubscription};
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentInputDelivery, MessageContent, SendAgentInputRequest};

use crate::api::ProtocolError;
use crate::ports::{AgentRunFailure, TurnRunHandle, TurnRunInput};

use super::OrchTurnRunner;
use super::completion::TurnCompletionScope;

impl OrchTurnRunner {
    pub(super) async fn run_execution_turn_subscription(
        &self,
        input: TurnRunInput,
        agent_spec: AgentSpec,
    ) -> Result<TurnRunHandle, ProtocolError> {
        let input_message_id = format!("msg_user_{}", uuid::Uuid::new_v4());
        let hub = Arc::new(orchd::events::SessionOutputHub::new(
            input.session_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            64,
        ));
        let root_agent_instance_id = format!("agent_{}_root", input.session_id);
        let prepared = self
            .prepare_session_runtime(
                &input.session_id,
                &input.cwd,
                &input.session_dir,
                &input.turn_id,
                &root_agent_instance_id,
                true,
                &agent_spec,
                input.resume_root_agent.as_ref(),
                Arc::clone(&hub),
            )
            .await?;

        let root_input = SendAgentInputRequest {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            session_id: input.session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: Some(input.turn_id.clone()),
            message_id: input_message_id,
            content: MessageContent::String(input.prompt.clone()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: Some(input.prompt_resources.clone()),
            active_tool_names: input.active_tool_names.clone(),
        };

        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            "agent runtime root Turn accepted"
        );

        {
            self.active_turns.lock().unwrap().insert(
                input.session_id.clone(),
                super::ActiveTurnRuntime {
                    turn_id: input.turn_id.clone(),
                    observation: Arc::clone(&hub),
                    root_agent_instance_id: root_agent_instance_id.clone(),
                    commit_router: Arc::clone(&prepared.commit_router),
                    realtime_router: Arc::clone(&prepared.realtime_router),
                },
            );
        }

        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&piko_protocol::agent_runtime::SessionCursor {
                epoch: cursor.epoch.clone(),
                seq: 0,
            })
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;

        let agent_runtime = Arc::clone(&self.agent_runtime);
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_instance_id = root_agent_instance_id.clone();
        let (completion_scope, completion) =
            TurnCompletionScope::new(session_id, turn_id, agent_instance_id, Arc::clone(&hub));
        tokio::spawn(async move {
            let result =
                agent_runtime
                    .run_agent(root_input)
                    .await
                    .map_err(|error| AgentRunFailure {
                        message: error.to_string(),
                    });
            completion_scope.complete(result);
        });

        Ok(TurnRunHandle {
            session_id: input.session_id.clone(),
            turn_id: input.turn_id,
            root_agent_instance_id: root_agent_instance_id.clone(),
            observation: SessionSubscription {
                session_id: input.session_id,
                cursor: cursor.clone(),
                output: orchd::events::merged_output_stream(hub_sub, cursor),
            },
            completion,
        })
    }
}

pub(super) fn root_agent_spec(cwd: impl AsRef<std::path::Path>) -> AgentSpec {
    let mut spec = crate::adapters::prompts::agent_loader::load_agents(cwd)
        .remove("main")
        .expect("built-in main agent must be registered");
    ensure_root_tool_sets(&mut spec);
    spec
}

/// Ensure root mandatory packs when a custom project agent omits them.
///
/// TOML remains the source of truth; this only adds missing
/// `user_interaction` / `multi_agent` for the live root turn.
pub(super) fn ensure_root_tool_sets(spec: &mut AgentSpec) {
    if !spec.tool_set_ids.iter().any(|id| id == "user_interaction") {
        spec.tool_set_ids.push("user_interaction".into());
    }
    if !spec.tool_set_ids.iter().any(|id| id == "multi_agent") {
        spec.tool_set_ids.push("multi_agent".into());
    }
}

/// Resolve the AgentSpec used while attaching a recovered AgentInstance.
///
/// Recovery always prefers the durable immutable AgentSpec snapshot. Registry
/// definitions are fallbacks only for legacy entries without a stored spec.
pub(super) fn resolve_recovered_agent_spec(
    agent_instance_id: &str,
    root_agent_instance_id: &str,
    stored_spec: Option<AgentSpec>,
    recovered_spec_id: &str,
    resolved_specs: &std::collections::HashMap<String, AgentSpec>,
    root_agent_spec: &AgentSpec,
) -> AgentSpec {
    stored_spec.unwrap_or_else(|| {
        if agent_instance_id == root_agent_instance_id {
            return root_agent_spec.clone();
        }
        resolved_specs
            .get(recovered_spec_id)
            .cloned()
            .or_else(|| {
                resolved_specs
                    .values()
                    .find(|spec| spec.id == recovered_spec_id)
                    .cloned()
            })
            .unwrap_or_else(|| root_agent_spec.clone())
    })
}
