//! Vertical-slice tests for the long-lived AgentInstance control plane.

#[path = "common/faux_provider.rs"]
mod faux_provider;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use piko_orchd::AgentRuntime;
use piko_orchd::testing::CollectingExecutionCommitPort;
use piko_orchd::tools::{MultiAgentToolProvider, UserInteractionProvider};
use piko_orchd_api::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, PromptAssemblyPort, SessionAgentConfig,
    SessionAgentPorts, SessionExecutionPorts, ToolExecutionContext, ToolProvider,
};
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInputDelivery, AgentInstanceIdentity,
    AgentInstanceLifecycle, AgentLifecycleRequest, AgentSpec, CommitError, CreateAgentRequest,
    MessageContent, PromptAssemblyRequest, SendAgentInputRequest,
};
use tokio::sync::Mutex;
use tokio::sync::Semaphore;

use faux_provider::{CannedResponse, FauxProvider};

/// Migration helper for request-shaped test fixtures. Product code has no
/// request adapter: every call below enters the canonical AgentInput API.
#[async_trait]
trait CanonicalRequestTestExt {
    async fn send_agent_input(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_orchd_api::AgentApiError>;

    async fn wait_sent_agent(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_protocol::AgentWorkReport, piko_orchd_api::AgentApiError>;
}

#[async_trait]
impl CanonicalRequestTestExt for AgentRuntime {
    async fn send_agent_input(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_orchd_api::AgentApiError> {
        self.submit_runtime_agent_input(
            piko_protocol::AgentInput::from_request(&request, 0),
            piko_orchd_api::AgentInputRuntime {
                prompt_resources: request.prompt_resources,
                active_tool_names: request.active_tool_names,
                source_turn_id: request.source_turn_id,
                message_id: Some(request.message_id),
            },
        )
        .await
    }

    async fn wait_sent_agent(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_protocol::AgentWorkReport, piko_orchd_api::AgentApiError> {
        let session_id = request.session_id.clone();
        let agent_instance_id = request.agent_instance_id.clone();
        let input_id = request.request_id.clone();
        self.send_agent_input(request).await?;
        self.wait_agent_input_completion(session_id, agent_instance_id, input_id)
            .await
    }
}

#[derive(Default)]
struct CollectingAgentCommitPort {
    revision: AtomicU64,
    commands: Mutex<Vec<AgentDurableCommand>>,
    fail_run_starts: AtomicU64,
    fail_queued_starts: AtomicU64,
    fail_run_terminals: AtomicU64,
    conflict_run_terminals: AtomicU64,
    terminal_attempts: AtomicU64,
    fail_report_commits: AtomicU64,
}

struct FailingMessageCommitPort {
    attempt: AtomicU64,
    fail_at: u64,
}

struct BlockingRunStartCommitPort {
    inner: Arc<CollectingAgentCommitPort>,
    entered: Semaphore,
    release: Semaphore,
}

struct PanicGateway;

#[derive(Default)]
struct RecordingPromptAssemblyPort {
    requests: Mutex<Vec<PromptAssemblyRequest>>,
}

#[async_trait]
impl PromptAssemblyPort for RecordingPromptAssemblyPort {
    async fn assemble_prompt(
        &self,
        request: PromptAssemblyRequest,
    ) -> Result<piko_protocol::SemanticRunPrompt, piko_orchd_api::AgentApiError> {
        let context = request
            .resources
            .blocks
            .iter()
            .map(|block| block.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let system_prompt = format!("{}|{}", request.agent_spec.base_instructions, context);
        self.requests.lock().await.push(request);
        Ok(piko_protocol::SemanticRunPrompt {
            source_digest: system_prompt.clone(),
            assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            blocks: vec![test_prompt_block(system_prompt)],
            cache_plan: Default::default(),
        })
    }
}

fn test_prompt_block(content: impl Into<String>) -> piko_protocol::PromptBlock {
    let content = content.into();
    piko_protocol::PromptBlock {
        id: "test".into(),
        kind: piko_protocol::PromptBlockKind::Instruction,
        authority: piko_protocol::InstructionAuthority::Agent,
        trust: piko_protocol::ContentTrust::Trusted,
        source: piko_protocol::PromptSource::new("test", "test"),
        content_digest: content.clone(),
        content,
        cache_scope: piko_protocol::CacheScope::NoCache,
    }
}

fn gateway_prompt_text(request: &piko_llmd::gateway::InferenceRequest) -> String {
    request
        .conversation
        .instructions
        .blocks
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

struct StrictCreateCommitPort {
    revision: AtomicU64,
    specs: Mutex<HashMap<String, AgentSpec>>,
}

impl StrictCreateCommitPort {
    fn with_agent(agent_instance_id: &str, spec: AgentSpec) -> Self {
        Self {
            revision: AtomicU64::new(0),
            specs: Mutex::new(HashMap::from([(agent_instance_id.into(), spec)])),
        }
    }
}

#[async_trait]
impl AgentCommitPort for StrictCreateCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, spec } => {
                let mut specs = self.specs.lock().await;
                match specs.get(&identity.agent_instance_id) {
                    Some(existing) if existing != &spec => {
                        return Err(CommitError::IdempotencyConflict);
                    }
                    Some(_) => {}
                    None => {
                        specs.insert(identity.agent_instance_id.clone(), spec);
                    }
                }
                identity.agent_instance_id
            }
            _ => String::new(),
        };
        Ok(AgentCommitAck {
            session_id: session_id.into(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}

#[async_trait]
impl piko_llmd::gateway::InferenceGateway for PanicGateway {
    async fn start(
        &self,
        _req: piko_llmd::gateway::InferenceRequest,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<piko_llmd::gateway::InferenceExecution, piko_llmd::gateway::InferenceError> {
        panic!("injected gateway panic")
    }
}

#[async_trait]
impl AgentCommitPort for BlockingRunStartCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        if matches!(&command, AgentDurableCommand::RunStarted { .. }) {
            self.entered.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("run start release semaphore closed")
                .forget();
        }
        self.inner.commit_agent_command(session_id, command).await
    }
}

#[async_trait]
impl piko_orchd_api::ExecutionCommitPort for FailingMessageCommitPort {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<piko_protocol::CommitAck, CommitError> {
        let attempt = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == self.fail_at {
            return Err(CommitError::Unavailable);
        }
        Ok(piko_protocol::CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: attempt,
        })
    }

    async fn commit_model_step(
        &self,
        commit: piko_protocol::execution::ModelStepCommit,
    ) -> Result<piko_protocol::CommitAck, CommitError> {
        let attempt = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == self.fail_at {
            return Err(CommitError::Unavailable);
        }
        Ok(piko_protocol::CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.assistant.message_id),
            revision: attempt,
        })
    }
}

impl CollectingAgentCommitPort {
    fn fail_next_run_start(&self) {
        self.fail_run_starts.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_next_queued_start(&self) {
        self.fail_queued_starts.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_next_run_terminal(&self) {
        self.fail_run_terminals.fetch_add(1, Ordering::SeqCst);
    }

    fn conflict_next_run_terminal(&self) {
        self.conflict_run_terminals.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_next_report_commit(&self) {
        self.fail_report_commits.fetch_add(1, Ordering::SeqCst);
    }

    fn consume_failure(counter: &AtomicU64) -> bool {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_sub(1)
            })
            .is_ok()
    }
}

#[async_trait]
impl AgentCommitPort for CollectingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        if matches!(&command, AgentDurableCommand::RunStarted { .. })
            && Self::consume_failure(&self.fail_run_starts)
        {
            return Err(CommitError::Unavailable);
        }
        if matches!(
            &command,
            AgentDurableCommand::RunStarted { input, .. }
                if input.delivery == AgentInputDelivery::FollowUp
        ) && Self::consume_failure(&self.fail_queued_starts)
        {
            return Err(CommitError::Unavailable);
        }
        if matches!(&command, AgentDurableCommand::RunTerminal { .. }) {
            self.terminal_attempts.fetch_add(1, Ordering::SeqCst);
        }
        if matches!(&command, AgentDurableCommand::RunTerminal { .. })
            && Self::consume_failure(&self.fail_run_terminals)
        {
            return Err(CommitError::Unavailable);
        }
        if matches!(&command, AgentDurableCommand::RunTerminal { .. })
            && Self::consume_failure(&self.conflict_run_terminals)
        {
            return Err(CommitError::IdempotencyConflict);
        }
        if matches!(&command, AgentDurableCommand::CommitReport { .. })
            && Self::consume_failure(&self.fail_report_commits)
        {
            return Err(CommitError::Unavailable);
        }
        let agent_instance_id = match &command {
            AgentDurableCommand::Create { identity, .. } => identity.agent_instance_id.clone(),
            AgentDurableCommand::AgentInputAdmitted { admission } => {
                admission.input.agent_instance_id.clone()
            }
            AgentDurableCommand::AgentInputDispositionChanged { change } => {
                change.agent_instance_id.clone()
            }
            AgentDurableCommand::SetLifecycle {
                agent_instance_id, ..
            }
            | AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id, ..
            } => agent_instance_id.clone(),
            AgentDurableCommand::RunTerminal { report, .. } => report.agent_instance_id.clone(),
            AgentDurableCommand::RunStarted {
                agent_instance_id, ..
            } => agent_instance_id.clone(),
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                ..
            } => recipient_agent_instance_id.clone(),
        };
        self.commands.lock().await.push(command);
        Ok(AgentCommitAck {
            session_id: session_id.to_string(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}

fn test_agent() -> AgentSpec {
    AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "main".into(),
        role: "test".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "test".into(),
        model: Some("faux-1".into()),
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    }
}

fn test_orchd_config() -> piko_protocol::config::OrchdConfig {
    let mut config = piko_protocol::config::OrchdConfig::default();
    config.default_model.provider = "faux".into();
    config.default_model.model_id = "faux-1".into();
    config.agents.insert("main".into(), test_agent());
    config
}

async fn attached_runtime_ports() -> (
    AgentRuntime,
    Arc<CollectingAgentCommitPort>,
    Arc<CollectingExecutionCommitPort>,
    Arc<FauxProvider>,
) {
    let model = Arc::new(FauxProvider::new());
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;
    let mut coder = test_agent();
    coder.id = "coder".into();
    coder.name = "coder".into();
    runtime.register_agent(coder).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-1".into(),
            root: AgentInstanceIdentity {
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>
                ),
            },
        })
        .await
        .expect("attach agent session");
    (runtime, agents, executions, model)
}

async fn attached_runtime() -> (
    AgentRuntime,
    Arc<CollectingAgentCommitPort>,
    Arc<FauxProvider>,
) {
    let (runtime, agents, _executions, model) = attached_runtime_ports().await;
    (runtime, agents, model)
}

async fn attached_bootstrapped_runtime_with_limits(
    max_agents: u32,
    max_depth: u32,
) -> (
    Arc<AgentRuntime>,
    Arc<CollectingAgentCommitPort>,
    Arc<FauxProvider>,
) {
    let model = Arc::new(FauxProvider::new());
    let mut config = test_orchd_config();
    config.runtime.max_agents = Some(max_agents);
    config.runtime.max_depth = Some(max_depth);
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        config,
    )
    .await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-limits".into(),
            root: AgentInstanceIdentity {
                session_id: "session-limits".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .expect("attach limited agent session");
    (runtime, agents, model)
}

include!("agent_runtime_cases/atomicity.rs");
include!("agent_runtime_cases/behavior.rs");
include!("agent_runtime_cases/context/mod.rs");
include!("agent_runtime_cases/multi_agent.rs");
include!("agent_runtime_cases/recovery.rs");
include!("agent_runtime_cases/recovery_delegation.rs");
include!("agent_runtime_cases/shutdown.rs");
include!("agent_runtime_cases/tool_sets.rs");
