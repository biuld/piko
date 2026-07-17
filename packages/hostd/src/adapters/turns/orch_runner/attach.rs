use std::sync::Arc;

use orchd_api::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, ExecutionCommitPort, RealtimeDeltaSink,
    SessionAgentConfig, SessionAgentPorts, SessionExecutionPorts,
};
use piko_protocol::agents::AgentSpec;

use crate::api::ProtocolError;
use crate::infra::storage::session_store::SessionStore;
use crate::ports::ResumeAgent;

use super::OrchAgentRunRunner;
use super::agent_commit::ProjectingAgentCommitPort;
use super::commit::{ExecutionCommitRouter, RealtimeDeltaRouter, RepositoryExecutionCommitPort};
use super::run::resolve_recovered_agent_spec;

pub(super) struct PreparedSessionRuntime {
    pub commit_router: Arc<ExecutionCommitRouter>,
    pub realtime_router: Arc<RealtimeDeltaRouter>,
}

impl OrchAgentRunRunner {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_session_runtime(
        &self,
        session_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        operation_id: &str,
        target_agent_instance_id: &str,
        fallback_route: bool,
        root_spec: &AgentSpec,
        resume_agent: Option<&ResumeAgent>,
        hub: Arc<orchd::events::SessionOutputHub>,
    ) -> Result<PreparedSessionRuntime, ProtocolError> {
        let attach_lock = self.session_attach_lock(session_id);
        let _attach_guard = attach_lock.lock().await;
        self.register_session_context(session_id.to_string(), cwd.to_string());
        let store = SessionStore::new(session_dir);
        let root = store
            .ensure_root_agent(&root_spec.id)
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        let durable_commit: Arc<dyn ExecutionCommitPort> =
            Arc::new(RepositoryExecutionCommitPort {
                store: store.clone(),
            });
        let commit_router = {
            let mut routers = self.commit_routers.lock().unwrap();
            Arc::clone(routers.entry(session_id.to_string()).or_insert_with(|| {
                Arc::new(ExecutionCommitRouter::new(
                    Arc::clone(&durable_commit),
                    store.clone(),
                ))
            }))
        };
        let realtime_router = {
            let mut routers = self.realtime_routers.lock().unwrap();
            Arc::clone(
                routers
                    .entry(session_id.to_string())
                    .or_insert_with(|| Arc::new(RealtimeDeltaRouter::default())),
            )
        };
        commit_router.register(
            target_agent_instance_id.to_string(),
            operation_id.to_string(),
            Arc::clone(&hub),
            fallback_route,
        );
        realtime_router.register(
            target_agent_instance_id.to_string(),
            operation_id.to_string(),
            hub,
            fallback_route,
        );

        if matches!(
            self.agent_runtime
                .agent_snapshot(session_id.to_string(), root.agent_instance_id.clone())
                .await,
            Err(orchd_api::AgentApiError::SessionNotAttached)
        ) {
            let agent_commit: Arc<dyn AgentCommitPort> = Arc::new(store.clone());
            let resolved_specs = crate::adapters::prompts::agent_loader::load_agents(cwd);
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
                        .agent_transcript(session_id, &agent_instance_id)
                        .unwrap_or_default();
                    let mut head_message_id = store
                        .load_agent(session_id, &agent_instance_id)
                        .ok()
                        .and_then(|agent| agent.head_message_id);
                    if agent_instance_id == root.agent_instance_id && transcript.is_empty() {
                        transcript = resume_agent
                            .map(|resume| resume.state.transcript.clone())
                            .unwrap_or_default();
                        head_message_id =
                            resume_agent.and_then(|resume| resume.state.head_message_id.clone());
                    }
                    AgentRecoveryState {
                        inbox: store.agent_inbox(&agent_instance_id).unwrap_or_default(),
                        identity: agent.identity,
                        spec: resolve_recovered_agent_spec(
                            &agent_instance_id,
                            &root.agent_instance_id,
                            agent.spec,
                            &recovered_spec_id,
                            &resolved_specs,
                            root_spec,
                        ),
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
                session_id.to_string(),
                &recovered_agents,
                Arc::clone(&self.ui_router),
            ));
            self.agent_runtime
                .attach_agent_session(SessionAgentConfig {
                    session_id: session_id.to_string(),
                    root,
                    recovered_agents,
                    ports: SessionAgentPorts {
                        agents: agent_commit,
                        executions: SessionExecutionPorts::new(
                            commit_router.clone() as Arc<dyn ExecutionCommitPort>
                        )
                        .with_prompt(Arc::new(super::prompt_assembly::HostPromptAssemblyPort))
                        .with_realtime(realtime_router.clone() as Arc<dyn RealtimeDeltaSink>),
                    },
                })
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        }

        Ok(PreparedSessionRuntime {
            commit_router,
            realtime_router,
        })
    }
}
