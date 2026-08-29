//! F-06 tool batch dispatch test doubles and harness.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use piko_orchd_api::SessionExecutionPorts;
use piko_orchd_api::tools::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use piko_protocol::execution::{ExecutionConfig, StartExecutionRequest};
use piko_protocol::tools::{ToolSet, ToolSetPolicy, ToolSetToolRef};
use piko_protocol::{MessageContent, Usage};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::adapters::persist::CollectingExecutionCommitPort;
use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolExecutorRef,
};
use crate::domain::tools::result::ToolExecResult;
use crate::runtime::execution::{AgentExecutionRuntime, ExecutionTerminal};
use piko_protocol::agents::AgentSpec;

/// Gateway that replays pre-queued event streams, one per model step.
#[derive(Default)]
pub(super) struct ToolCallingGateway {
    responses: Mutex<VecDeque<Vec<InferenceEvent>>>,
    call_count: AtomicU32,
}

impl ToolCallingGateway {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push_step(&self, events: Vec<InferenceEvent>) {
        self.responses.lock().unwrap().push_back(events);
    }

    pub(super) fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl InferenceGateway for ToolCallingGateway {
    async fn start(
        &self,
        _req: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let events = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| vec![InferenceEvent::completed("stop")]);
        Ok(InferenceExecution {
            events: Box::pin(tokio_stream::iter(events)),
            handle: None,
        })
    }
}

/// One model step that emits the given tool calls and stops with `tool_use`.
pub(super) fn tool_use_step(calls: &[(&str, &str)]) -> Vec<InferenceEvent> {
    let mut events = vec![InferenceEvent::text(String::new())];
    for (id, name) in calls {
        events.push(InferenceEvent::function_call(*id, *name, "{}"));
    }
    events.push(InferenceEvent::Usage(Usage::empty()));
    events.push(InferenceEvent::completed("tool_use"));
    events
}

pub(super) fn text_step(text: &str) -> Vec<InferenceEvent> {
    vec![
        InferenceEvent::text(text),
        InferenceEvent::Usage(Usage::empty()),
        InferenceEvent::completed("stop"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimingPhase {
    Started,
    Finished,
}

#[derive(Debug, Clone)]
pub(super) struct TimingEvent {
    pub(super) tool: String,
    pub(super) phase: TimingPhase,
    pub(super) at: Instant,
}

struct TimingToolState {
    delays: Arc<Mutex<HashMap<String, Duration>>>,
    events: Arc<Mutex<Vec<TimingEvent>>>,
    inflight: AtomicU32,
    max_concurrent: AtomicU32,
    execution_counts: Arc<Mutex<HashMap<String, u32>>>,
    hold_enabled: AtomicBool,
    entered: Arc<Semaphore>,
    release: Arc<Notify>,
}

/// Timing-aware fake tool provider: per-tool delays, an optional hold that
/// parks executions until released, and shared concurrency observations.
#[derive(Clone)]
pub(super) struct TimingToolProvider(Arc<TimingToolState>);

impl TimingToolProvider {
    pub(super) fn new() -> Self {
        Self(Arc::new(TimingToolState {
            delays: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            inflight: AtomicU32::new(0),
            max_concurrent: AtomicU32::new(0),
            execution_counts: Arc::new(Mutex::new(HashMap::new())),
            hold_enabled: AtomicBool::new(false),
            entered: Arc::new(Semaphore::new(0)),
            release: Arc::new(Notify::new()),
        }))
    }

    pub(super) fn set_delay(&self, tool: &str, delay: Duration) {
        self.0.delays.lock().unwrap().insert(tool.into(), delay);
    }

    pub(super) fn enable_hold(&self) {
        self.0.hold_enabled.store(true, Ordering::SeqCst);
    }

    /// Wait until `n` executions have entered (and are parked when held).
    pub(super) async fn wait_entered(&self, n: u32) {
        for _ in 0..n {
            let permit = self
                .0
                .entered
                .clone()
                .acquire_owned()
                .await
                .expect("entered semaphore closed");
            drop(permit);
        }
    }

    pub(super) fn max_concurrent(&self) -> u32 {
        self.0.max_concurrent.load(Ordering::SeqCst)
    }

    pub(super) fn events(&self) -> Vec<TimingEvent> {
        self.0.events.lock().unwrap().clone()
    }

    pub(super) fn execution_count(&self, tool: &str) -> u32 {
        self.0
            .execution_counts
            .lock()
            .unwrap()
            .get(tool)
            .copied()
            .unwrap_or(0)
    }
}

fn timing_tool(name: &str, mode: ToolExecutionMode) -> ToolDef {
    ToolDef {
        name: name.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", name),
        description: String::new(),
        input_schema: serde_json::json!({}),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: name.into(),
            extra: None,
        },
        execution_mode: Some(mode),
        exposure: None,
        capabilities: None,
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

fn timing_tools() -> Vec<ToolDef> {
    vec![
        timing_tool("par_a", ToolExecutionMode::Parallel),
        timing_tool("par_b", ToolExecutionMode::Parallel),
        timing_tool("seq_c", ToolExecutionMode::Sequential),
        timing_tool("seq_d", ToolExecutionMode::Sequential),
    ]
}

#[async_trait]
impl ToolProvider for TimingToolProvider {
    fn id(&self) -> &str {
        "timing"
    }

    fn source(&self) -> piko_protocol::tools::ToolProviderSource {
        piko_protocol::tools::ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        timing_tools()
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        let state = self.0.clone();
        let tool = call.name.clone();
        {
            let mut counts = state.execution_counts.lock().unwrap();
            *counts.entry(tool.clone()).or_insert(0) += 1;
        }
        state.events.lock().unwrap().push(TimingEvent {
            tool: tool.clone(),
            phase: TimingPhase::Started,
            at: Instant::now(),
        });
        let current = state.inflight.fetch_add(1, Ordering::SeqCst) + 1;
        state.max_concurrent.fetch_max(current, Ordering::SeqCst);

        let delay = state.delays.lock().unwrap().get(&tool).copied();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        if state.hold_enabled.load(Ordering::SeqCst) {
            state.entered.add_permits(1);
            state.release.notified().await;
        }

        state.inflight.fetch_sub(1, Ordering::SeqCst);
        state.events.lock().unwrap().push(TimingEvent {
            tool,
            phase: TimingPhase::Finished,
            at: Instant::now(),
        });
        ToolExecResult {
            ok: true,
            value: Some(serde_json::json!(format!("result-{}", call.name))),
            error: None,
        }
    }
}

/// Runtime harness with a timing provider + workspace-less tool set.
pub(super) struct ToolBatchHarness {
    pub(super) runtime: Arc<AgentExecutionRuntime>,
    pub(super) provider: Arc<TimingToolProvider>,
    pub(super) commits: Arc<CollectingExecutionCommitPort>,
}

pub(super) async fn tool_batch_harness(
    gateway: Arc<ToolCallingGateway>,
    set_policy: Option<ToolSetPolicy>,
) -> ToolBatchHarness {
    let runtime = Arc::new(AgentExecutionRuntime::new(
        gateway as Arc<dyn InferenceGateway>,
    ));
    let provider = Arc::new(TimingToolProvider::new());
    runtime
        .register_tool_provider(Box::new((*provider).clone()))
        .await;
    runtime
        .register_tool_set(ToolSet {
            id: "timing".into(),
            name: "Timing Tools".into(),
            description: None,
            feature: None,
            metadata: None,
            policy: set_policy,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "timing".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        })
        .await;
    let commits = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(
            "session-batch".into(),
            SessionExecutionPorts::new(
                commits.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>
            ),
        )
        .await
        .unwrap();
    ToolBatchHarness {
        runtime,
        provider,
        commits,
    }
}

pub(super) async fn discover_batch_routes(
    runtime: &Arc<AgentExecutionRuntime>,
) -> (Vec<ToolDef>, HashMap<String, CatalogRoute>) {
    runtime
        .services()
        .tool_registry()
        .discover_tools(&ToolDiscoveryContext {
            agent_id: "main".into(),
            agent_kind: piko_protocol::AgentKind::Supervisor,
            agent_instance_id: Some("agent-batch".into()),
            tool_set_ids: vec!["timing".into()],
            active_tool_names: None,
        })
        .await
        .unwrap()
}

pub(super) fn batch_request(execution_id: &str, tools: Vec<ToolDef>) -> StartExecutionRequest {
    StartExecutionRequest {
        request_id: "request-batch".into(),
        session_id: "session-batch".into(),
        source_turn_id: None,
        execution_id: execution_id.into(),
        agent_instance_id: "agent-batch".into(),
        agent_spec: AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "main".into(),
            role: "test".into(),
            kind: piko_protocol::AgentKind::Supervisor,
            description: None,
            base_instructions: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["timing".into()],
            active_tool_names: None,
        },
        run_prompt: piko_protocol::SemanticRunPrompt {
            assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            source_digest: "digest".into(),
            ..Default::default()
        },
        tool_catalog: piko_protocol::ResolvedToolCatalog::new(tools, "digest"),
        world_state: None,
        inter_agent_completions: Vec::new(),
        user_mentions: Vec::new(),
        input_message_id: "message-batch".into(),
        input: MessageContent::String("run tools".into()),
        context: piko_protocol::ConversationContext::empty(),
        config: ExecutionConfig {
            agent_id: "main".into(),
            ..Default::default()
        },
    }
}

/// Run a tool batch to completion and return the actor transcript.
pub(super) async fn run_batch(
    runtime: &Arc<AgentExecutionRuntime>,
    execution_id: &str,
    tools: Vec<ToolDef>,
    routes: HashMap<String, CatalogRoute>,
) -> ExecutionTerminal {
    let prepared = runtime
        .prepare_execution(
            batch_request(execution_id, tools),
            routes,
            tracing::Span::none(),
        )
        .await
        .unwrap();
    prepared.activate().await;
    runtime
        .wait_terminal_state("session-batch", execution_id)
        .await
        .unwrap()
}
