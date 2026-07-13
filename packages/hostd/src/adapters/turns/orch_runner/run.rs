use std::sync::Arc;

use orchd_api::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, ExecutionCommitPort, RealtimeDeltaSink,
    SessionAgentConfig, SessionAgentPorts, SessionExecutionPorts, SessionSubscription,
};
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentInputDelivery, MessageContent, SendAgentInputRequest};

use crate::adapters::turns::notifying_execution_commit::NotifyingExecutionCommitPort;
use crate::api::ProtocolError;
use crate::infra::storage::session_store::SessionStore;
use crate::ports::{AgentRunFailure, TurnRunHandle, TurnRunInput};

use super::OrchTurnRunner;
use super::agent_commit::ProjectingAgentCommitPort;
use super::commit::{ExecutionCommitRouter, RealtimeDeltaRouter, RepositoryExecutionCommitPort};
use super::completion::TurnCompletionScope;

impl OrchTurnRunner {
    pub(super) async fn run_execution_turn_subscription(
        &self,
        input: TurnRunInput,
        agent_spec: AgentSpec,
    ) -> Result<TurnRunHandle, ProtocolError> {
        self.register_session_context(input.session_id.clone(), input.cwd.clone());
        let input_message_id = format!("msg_user_{}", uuid::Uuid::new_v4());
        let hub = Arc::new(orchd::testing::SessionOutputHub::new(
            input.session_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            64,
        ));
        let root_agent_instance_id = format!("agent_{}_root", input.session_id);
        let store = SessionStore::new(&input.session_dir);
        store
            .ensure_root_agent(&agent_spec.id)
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        let inner_commit: Arc<dyn ExecutionCommitPort> = Arc::new(RepositoryExecutionCommitPort {
            store: store.clone(),
        });

        let commit: Arc<dyn ExecutionCommitPort> = Arc::new(NotifyingExecutionCommitPort::new(
            Arc::clone(&inner_commit),
            Arc::clone(&hub),
            agent_spec.id.clone(),
        ));

        let router =
            {
                let mut routers = self.commit_routers.lock().unwrap();
                Arc::clone(routers.entry(input.session_id.clone()).or_insert_with(|| {
                    Arc::new(ExecutionCommitRouter::new(Arc::clone(&inner_commit)))
                }))
            };
        router.install(commit);
        let realtime_router = {
            let mut routers = self.realtime_routers.lock().unwrap();
            Arc::clone(
                routers
                    .entry(input.session_id.clone())
                    .or_insert_with(|| Arc::new(RealtimeDeltaRouter::default())),
            )
        };
        realtime_router.install(input.turn_id.clone(), Arc::clone(&hub));

        if matches!(
            self.agent_runtime
                .agent_snapshot(input.session_id.clone(), root_agent_instance_id.clone())
                .await,
            Err(orchd_api::AgentApiError::SessionNotAttached)
        ) {
            let root = store
                .ensure_root_agent(&agent_spec.id)
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            let agent_commit: Arc<dyn AgentCommitPort> = Arc::new(store.clone());
            let resolved_specs = crate::adapters::prompts::agent_loader::load_agents(&input.cwd);
            store
                .interrupt_incomplete_agent_executions()
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            let recovered_agents: Vec<AgentRecoveryState> = store
                .agent_instances()
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
                .into_iter()
                .map(|agent| {
                    let agent_instance_id = agent.identity.agent_instance_id.clone();
                    let recovered_spec_id = agent.identity.agent_spec_id.clone();
                    let mut transcript = store
                        .agent_transcript(&input.session_id, &agent_instance_id)
                        .unwrap_or_default();
                    let mut head_message_id = store
                        .load_agent(&input.session_id, &agent_instance_id)
                        .ok()
                        .and_then(|agent| agent.head_message_id);
                    if agent_instance_id == root.agent_instance_id && transcript.is_empty() {
                        transcript = input
                            .resume_root_agent
                            .as_ref()
                            .map(|resume| resume.state.transcript.clone())
                            .unwrap_or_default();
                        head_message_id = input
                            .resume_root_agent
                            .as_ref()
                            .and_then(|resume| resume.state.head_message_id.clone());
                    }
                    AgentRecoveryState {
                        inbox: store.agent_inbox(&agent_instance_id).unwrap_or_default(),
                        identity: agent.identity,
                        spec: agent.spec.unwrap_or_else(|| {
                            resolved_specs
                                .get(&recovered_spec_id)
                                .cloned()
                                .or_else(|| {
                                    resolved_specs
                                        .values()
                                        .find(|spec| spec.id == recovered_spec_id)
                                        .cloned()
                                })
                                .unwrap_or_else(|| agent_spec.clone())
                        }),
                        lifecycle: agent.lifecycle,
                        transcript,
                        head_message_id,
                        latest_report: agent.latest_report,
                        execution_reports: store
                            .agent_execution_reports(&agent_instance_id)
                            .unwrap_or_default(),
                        queued_inputs: store
                            .agent_queued_inputs(&agent_instance_id)
                            .unwrap_or_default(),
                        pending_detached_deliveries: store
                            .pending_detached_deliveries(&agent_instance_id)
                            .unwrap_or_default(),
                    }
                })
                .collect();
            let agent_commit: Arc<dyn AgentCommitPort> = Arc::new(ProjectingAgentCommitPort::new(
                agent_commit,
                &recovered_agents,
                Arc::clone(&self.agent_event_tx),
            ));
            self.agent_runtime
                .attach_agent_session(SessionAgentConfig {
                    session_id: input.session_id.clone(),
                    root,
                    recovered_agents,
                    ports: SessionAgentPorts {
                        agents: agent_commit,
                        executions: SessionExecutionPorts::new(
                            router.clone() as Arc<dyn ExecutionCommitPort>
                        )
                        .with_realtime(realtime_router as Arc<dyn RealtimeDeltaSink>),
                    },
                })
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        }

        let root_input = SendAgentInputRequest {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            session_id: input.session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: Some(input.turn_id.clone()),
            message_id: input_message_id,
            content: MessageContent::String(input.prompt.clone()),
            delivery: AgentInputDelivery::StartWhenIdle,
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
                    durable_commit: Arc::clone(&inner_commit),
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
        let agent_instance_id = root_agent_instance_id;
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
            observation: SessionSubscription {
                session_id: input.session_id,
                cursor: cursor.clone(),
                output: orchd::testing::merged_output_stream(hub_sub, cursor),
            },
            completion,
        })
    }
}

pub(super) fn root_agent_spec(
    cwd: impl AsRef<std::path::Path>,
    system_prompt: String,
    active_tool_names: Option<Vec<String>>,
) -> AgentSpec {
    let mut spec = crate::adapters::prompts::agent_loader::load_agents(cwd)
        .remove("main")
        .expect("built-in main agent must be registered");
    spec.system_prompt = system_prompt;
    if !spec.tool_set_ids.iter().any(|id| id == "user_interaction") {
        spec.tool_set_ids.push("user_interaction".into());
    }
    if !spec.tool_set_ids.iter().any(|id| id == "multi_agent") {
        spec.tool_set_ids.push("multi_agent".into());
    }
    spec.active_tool_names = active_tool_names;
    spec
}
