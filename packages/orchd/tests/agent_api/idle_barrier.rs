//! Regression tests for step idle barrier vs steer / resume / concurrent submit.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use futures_core::Stream;
use orchd::AgentRuntimeService;
use orchd::api::AgentRuntime;
use orchd::testing::CollectingPersistSink;
use orchd::testing::Supervisor;
use orchd::testing::detach_task_runtime;
use orchd_api::{
    MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
};
use piko_protocol::MessageContent;
use piko_protocol::TaskEvent;
use piko_protocol::agent_runtime::{InputDelivery, InputSource, SubmitTaskInput, TaskResumeState};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use super::support::{sample_create_request, test_agent_spec, test_config};
use crate::faux_provider::FauxProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PersistSlotKind {
    Message,
    TaskEvent,
    WorkEvent,
}

/// Enforces hostd-style task_seq uniqueness across message / lifecycle / work records.
struct StrictSeqPersistSink {
    inner: CollectingPersistSink,
    slots: StdMutex<HashMap<u64, PersistSlotKind>>,
}

impl StrictSeqPersistSink {
    fn new() -> Self {
        Self {
            inner: CollectingPersistSink::new(),
            slots: StdMutex::new(HashMap::new()),
        }
    }

    fn inner(&self) -> &CollectingPersistSink {
        &self.inner
    }

    fn reserve_slot(&self, task_seq: u64, kind: PersistSlotKind) -> Result<(), PersistError> {
        let mut slots = self.slots.lock().expect("strict seq lock");
        match slots.get(&task_seq) {
            None => {
                slots.insert(task_seq, kind);
                Ok(())
            }
            Some(existing) if *existing == kind => Ok(()),
            Some(_) => Err(PersistError::IdempotencyConflict),
        }
    }
}

#[async_trait]
impl PersistSink for StrictSeqPersistSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        self.reserve_slot(event.task_seq, PersistSlotKind::Message)?;
        self.inner.commit_message(event).await
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        self.reserve_slot(event.task_seq, PersistSlotKind::TaskEvent)?;
        self.inner.commit_task_event(event).await
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        self.reserve_slot(event.task_seq, PersistSlotKind::WorkEvent)?;
        self.inner.commit_work_event(event).await
    }
}

/// Rejects TaskEvent::Idle commits while forwarding everything else.
struct RejectIdleLifecycleSink {
    inner: Arc<dyn PersistSink>,
}

#[async_trait]
impl PersistSink for RejectIdleLifecycleSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_message(event).await
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        if matches!(event.event, TaskEvent::Idle { .. }) {
            return Err(PersistError::IdempotencyConflict);
        }
        self.inner.commit_task_event(event).await
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_work_event(event).await
    }
}

/// Passes the first `allow` Idle commits, then blocks the next Idle until `release()`.
struct GatedIdlePersistSink {
    inner: Arc<dyn PersistSink>,
    gate: Arc<Notify>,
    allow: StdMutex<u32>,
    idle_blocked: Arc<Mutex<bool>>,
}

impl GatedIdlePersistSink {
    fn allowing(inner: Arc<dyn PersistSink>, allow: u32) -> Self {
        Self {
            inner,
            gate: Arc::new(Notify::new()),
            allow: StdMutex::new(allow),
            idle_blocked: Arc::new(Mutex::new(false)),
        }
    }

    fn release(&self) {
        self.gate.notify_waiters();
    }

    async fn wait_until_idle_blocked(&self) {
        let blocked = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if *self.idle_blocked.lock().await {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(blocked.is_ok(), "timed out waiting for idle persist gate");
    }
}

#[async_trait]
impl PersistSink for GatedIdlePersistSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_message(event).await
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        if matches!(event.event, TaskEvent::Idle { .. }) {
            let should_block = {
                let mut allow = self.allow.lock().expect("allow lock");
                if *allow > 0 {
                    *allow -= 1;
                    false
                } else {
                    true
                }
            };
            if should_block {
                *self.idle_blocked.lock().await = true;
                self.gate.notified().await;
            }
        }
        self.inner.commit_task_event(event).await
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_work_event(event).await
    }
}

/// Ordered persist record for sequence assertions.
#[derive(Debug, Clone)]
enum PersistRecord {
    UserMessage { task_seq: u64, message_id: String },
    AssistantMessage { task_seq: u64, message_id: String },
    TaskEvent { task_seq: u64, kind: &'static str },
    WorkEvent { task_seq: u64, _status: String },
}

fn collect_ordered_records(sink: &CollectingPersistSink) -> Vec<PersistRecord> {
    let mut records = Vec::new();
    for commit in sink.messages() {
        let kind = match &commit.message {
            piko_protocol::Message::User { .. } => PersistRecord::UserMessage {
                task_seq: commit.task_seq,
                message_id: commit.message_id.clone(),
            },
            piko_protocol::Message::Assistant { .. } => PersistRecord::AssistantMessage {
                task_seq: commit.task_seq,
                message_id: commit.message_id.clone(),
            },
            _ => continue,
        };
        records.push(kind);
    }
    for commit in sink.task_events() {
        let kind = match &commit.event {
            TaskEvent::Idle { .. } => "idle",
            TaskEvent::Steered { .. } => "steered",
            TaskEvent::Started { .. } => "started",
            TaskEvent::Failed { .. } => "failed",
            TaskEvent::Created { .. } => "created",
            _ => "other",
        };
        records.push(PersistRecord::TaskEvent {
            task_seq: commit.task_seq,
            kind,
        });
    }
    for commit in sink.work_events() {
        records.push(PersistRecord::WorkEvent {
            task_seq: commit.task_seq,
            _status: format!("{:?}", commit.snapshot.status),
        });
    }
    records.sort_by_key(|record| match record {
        PersistRecord::UserMessage { task_seq, .. }
        | PersistRecord::AssistantMessage { task_seq, .. }
        | PersistRecord::TaskEvent { task_seq, .. }
        | PersistRecord::WorkEvent { task_seq, .. } => *task_seq,
    });
    records
}

fn task_seq(record: &PersistRecord) -> u64 {
    match record {
        PersistRecord::UserMessage { task_seq, .. }
        | PersistRecord::AssistantMessage { task_seq, .. }
        | PersistRecord::TaskEvent { task_seq, .. }
        | PersistRecord::WorkEvent { task_seq, .. } => *task_seq,
    }
}

async fn wait_for_idle(sink: &CollectingPersistSink, task_id: &str) {
    let found = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if sink.task_events().iter().any(|commit| {
                commit.task_id == task_id && matches!(commit.event, TaskEvent::Idle { .. })
            }) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(found.is_ok(), "timed out waiting for TaskEvent::Idle");
}

/// Blocks the first chat_stream call until `release()` is invoked.
struct GatedFauxProvider {
    inner: FauxProvider,
    gate: Arc<tokio::sync::Notify>,
    first_call_started: Arc<Mutex<bool>>,
}

impl GatedFauxProvider {
    fn new() -> Self {
        Self {
            inner: FauxProvider::new(),
            gate: Arc::new(tokio::sync::Notify::new()),
            first_call_started: Arc::new(Mutex::new(false)),
        }
    }

    async fn push_text(&self, text: impl Into<String>) {
        self.inner.push_text(text).await;
    }

    fn release(&self) {
        self.gate.notify_waiters();
    }

    async fn wait_until_stream_started(&self) {
        let started = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if *self.first_call_started.lock().await {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(started.is_ok(), "timed out waiting for LLM stream to start");
    }
}

#[async_trait]
impl llmd::gateway::LlmGateway for GatedFauxProvider {
    async fn chat_stream(
        &self,
        req: llmd::gateway::GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = llmd::gateway::GatewayEvent> + Send + 'static>>, String>
    {
        *self.first_call_started.lock().await = true;
        self.gate.notified().await;
        self.inner.chat_stream(req, cancel).await
    }

    async fn llm_call(
        &self,
        model: piko_protocol::messages::Model,
        system_prompt: Option<String>,
        messages: Vec<piko_protocol::messages::Message>,
        settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        self.inner
            .llm_call(model, system_prompt, messages, settings)
            .await
    }

    fn capabilities(&self) -> piko_protocol::model::ModelCapabilities {
        self.inner.capabilities()
    }
}

#[tokio::test]
async fn follow_up_turn_persists_idle_before_next_user_message() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("second").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    runtime
        .submit_input(SubmitTaskInput {
            request_id: "req-1".into(),
            session_id: "session-idem".into(),
            task_id: handle.task_id.clone(),
            message_id: "msg-user-1".into(),
            work_id: "work-1".into(),
            source_turn_id: Some("turn-1".into()),
            source: InputSource::User,
            content: MessageContent::String("hello".into()),
            delivery: InputDelivery::AfterCurrentStep,
            submitted_at: 1,
        })
        .await
        .unwrap();
    wait_for_idle(&sink, &handle.task_id).await;

    runtime
        .submit_input(SubmitTaskInput {
            request_id: "req-2".into(),
            session_id: "session-idem".into(),
            task_id: handle.task_id.clone(),
            message_id: "msg-user-2".into(),
            work_id: "work-2".into(),
            source_turn_id: Some("turn-2".into()),
            source: InputSource::User,
            content: MessageContent::String("follow up".into()),
            delivery: InputDelivery::AfterCurrentStep,
            submitted_at: 2,
        })
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let records = collect_ordered_records(&sink);
    let assistant_1 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::AssistantMessage { message_id, .. } if message_id.contains("step_1")
            )
        })
        .expect("first assistant commit");
    let idle = records
        .iter()
        .find(|record| matches!(record, PersistRecord::TaskEvent { kind: "idle", .. }))
        .expect("idle lifecycle commit");
    let user_2 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::UserMessage {
                    message_id,
                    ..
                } if message_id == "msg-user-2"
            )
        })
        .expect("second user commit");

    assert!(
        task_seq(assistant_1) < task_seq(idle),
        "idle must follow first assistant; records={records:?}"
    );
    assert!(
        task_seq(idle) < task_seq(user_2),
        "second user must not precede idle; records={records:?}"
    );
}

#[tokio::test]
async fn resumed_runtime_waits_then_accepts_follow_up_without_recreating_task() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("after resume").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let mut request = sample_create_request();
    request.resume = Some(TaskResumeState {
        transcript: vec![piko_protocol::Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Text {
                text: "prior assistant".into(),
            }],
            api: "test".into(),
            provider: "faux".into(),
            model: "faux-1".into(),
            usage: None,
            stop_reason: Some("stop".into()),
            error_message: None,
            timestamp: Some(1),
        }],
        head_message_id: Some("msg-assistant-prior".into()),
        last_task_seq: 6,
        committed_message_ids: vec!["msg-assistant-prior".into()],
    });
    let handle = runtime.create_task(request).await.unwrap();

    runtime
        .submit_input(SubmitTaskInput {
            request_id: "req-resume-1".into(),
            session_id: "session-idem".into(),
            task_id: handle.task_id.clone(),
            message_id: "msg-user-resume".into(),
            work_id: "work-resume".into(),
            source_turn_id: Some("turn-resume".into()),
            source: InputSource::User,
            content: MessageContent::String("steer after resume".into()),
            delivery: InputDelivery::AfterCurrentStep,
            submitted_at: 2,
        })
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let records = collect_ordered_records(&sink);
    let user = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::UserMessage {
                    message_id,
                    ..
                } if message_id == "msg-user-resume"
            )
        })
        .expect("follow-up user commit");
    assert_eq!(
        task_seq(user),
        7,
        "follow-up user should continue resumed sequence; records={records:?}"
    );
    assert!(
        records.iter().any(|record| matches!(
            record,
            PersistRecord::TaskEvent {
                kind: "started",
                ..
            }
        )),
        "resumed follow-up must start a new work; records={records:?}"
    );
    assert!(
        sink.task_events()
            .iter()
            .all(|commit| !matches!(commit.event, TaskEvent::Created { .. })),
        "resume must not recreate the task; records={records:?}"
    );
}

#[tokio::test]
async fn submit_during_active_step_is_persisted_after_idle_when_llm_still_running() {
    let gated = Arc::new(GatedFauxProvider::new());
    gated.push_text("blocked then done").await;
    gated.push_text("second step").await;
    let core = Supervisor::from_config(
        Arc::clone(&gated) as Arc<dyn llmd::gateway::LlmGateway>,
        test_config(),
    )
    .await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    let first = runtime
        .submit_input(SubmitTaskInput {
            request_id: "req-active-1".into(),
            session_id: "session-idem".into(),
            task_id: handle.task_id.clone(),
            message_id: "msg-active-1".into(),
            work_id: "work-active-1".into(),
            source_turn_id: Some("turn-a".into()),
            source: InputSource::User,
            content: MessageContent::String("first".into()),
            delivery: InputDelivery::AfterCurrentStep,
            submitted_at: 1,
        })
        .await
        .unwrap();
    assert_eq!(
        first.disposition,
        piko_protocol::agent_runtime::InputDisposition::Accepted
    );

    gated.wait_until_stream_started().await;

    let core_bg = Arc::clone(&core);
    let task_id = handle.task_id.clone();
    let second_submit = tokio::spawn(async move {
        let runtime_bg = AgentRuntimeService::new(core_bg);
        runtime_bg
            .submit_input(SubmitTaskInput {
                request_id: "req-active-2".into(),
                session_id: "session-idem".into(),
                task_id,
                message_id: "msg-active-2".into(),
                work_id: "work-active-2".into(),
                source_turn_id: Some("turn-b".into()),
                source: InputSource::User,
                content: MessageContent::String("second".into()),
                delivery: InputDelivery::AfterCurrentStep,
                submitted_at: 2,
            })
            .await
    });

    gated.release();
    let second = tokio::time::timeout(std::time::Duration::from_secs(2), second_submit)
        .await
        .expect("second submit timed out")
        .expect("second submit join failed")
        .expect("second submit failed");
    assert_eq!(
        second.disposition,
        piko_protocol::agent_runtime::InputDisposition::Queued
    );

    wait_for_idle(&sink, &handle.task_id).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let records = collect_ordered_records(&sink);
    let assistant_1 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::AssistantMessage { message_id, .. } if message_id.contains("step_1")
            )
        })
        .expect("first assistant");
    let idle = records
        .iter()
        .find(|record| matches!(record, PersistRecord::TaskEvent { kind: "idle", .. }))
        .expect("idle");
    let user_2 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::UserMessage {
                    message_id,
                    ..
                } if message_id == "msg-active-2"
            )
        })
        .expect("second user");

    assert!(
        task_seq(assistant_1) < task_seq(idle) && task_seq(idle) < task_seq(user_2),
        "queued follow-up must wait for idle; records={records:?}"
    );
}

fn last_persisted_task_seq(sink: &CollectingPersistSink) -> u64 {
    [
        sink.messages().iter().map(|commit| commit.task_seq).max(),
        sink.task_events()
            .iter()
            .map(|commit| commit.task_seq)
            .max(),
        sink.work_events()
            .iter()
            .map(|commit| commit.task_seq)
            .max(),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0)
}

fn resume_state_from_sink(sink: &CollectingPersistSink) -> TaskResumeState {
    TaskResumeState {
        transcript: sink
            .messages()
            .iter()
            .map(|commit| commit.message.clone())
            .collect(),
        head_message_id: sink
            .messages()
            .last()
            .map(|commit| commit.message_id.clone()),
        last_task_seq: last_persisted_task_seq(sink),
        committed_message_ids: sink
            .messages()
            .iter()
            .map(|commit| commit.message_id.clone())
            .collect(),
    }
}

fn submit_turn(
    task_id: &str,
    request_id: &str,
    message_id: &str,
    work_id: &str,
    turn_id: &str,
    prompt: &str,
    submitted_at: i64,
) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: request_id.into(),
        session_id: "session-idem".into(),
        task_id: task_id.into(),
        message_id: message_id.into(),
        work_id: work_id.into(),
        source_turn_id: Some(turn_id.into()),
        source: InputSource::User,
        content: MessageContent::String(prompt.into()),
        delivery: InputDelivery::AfterCurrentStep,
        submitted_at,
    }
}

async fn run_single_turn(runtime: &AgentRuntimeService, task_id: &str, turn: u32, prompt: &str) {
    runtime
        .submit_input(submit_turn(
            task_id,
            &format!("req-turn-{turn}"),
            &format!("msg-user-{turn}"),
            &format!("work-{turn}"),
            &format!("turn-{turn}"),
            prompt,
            i64::from(turn),
        ))
        .await
        .expect("turn submit");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

#[tokio::test]
async fn strict_seq_sink_allows_normal_follow_up_turns() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("second").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let strict = Arc::new(StrictSeqPersistSink::new());
    core.set_persist_sink(strict.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "first").await;
    run_single_turn(&runtime, &handle.task_id, 2, "second").await;
    wait_for_idle(strict.inner(), &handle.task_id).await;
}

#[tokio::test]
async fn idle_lifecycle_rejection_stops_runtime_before_follow_up() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("second").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let strict = Arc::new(StrictSeqPersistSink::new());
    let sink = Arc::new(RejectIdleLifecycleSink {
        inner: strict.clone() as Arc<dyn PersistSink>,
    });
    core.set_persist_sink(sink as Arc<dyn PersistSink>).await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "first").await;

    let second = runtime
        .submit_input(submit_turn(
            &handle.task_id,
            "req-turn-2",
            "msg-user-2",
            "work-2",
            "turn-2",
            "second",
            2,
        ))
        .await;
    assert!(
        matches!(second, Err(orchd::api::AgentApiError::TaskNotFound)),
        "idle rejection stops runtime before another follow-up can attach; second={second:?}"
    );

    let records = collect_ordered_records(strict.inner());
    assert!(
        !records
            .iter()
            .any(|record| matches!(record, PersistRecord::TaskEvent { kind: "idle", .. })),
        "idle lifecycle must not durably commit when rejected; records={records:?}"
    );
    assert!(
        records.iter().any(|record| {
            matches!(
                record,
                PersistRecord::AssistantMessage { message_id, .. }
                    if message_id.contains("step_1")
            )
        }),
        "first assistant must still persist; records={records:?}"
    );
}

#[tokio::test]
async fn respawn_after_idle_rejection_skips_idle_before_follow_up() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("after respawn").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let strict = Arc::new(StrictSeqPersistSink::new());
    let sink = Arc::new(RejectIdleLifecycleSink {
        inner: strict.clone() as Arc<dyn PersistSink>,
    });
    core.set_persist_sink(sink as Arc<dyn PersistSink>).await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "first").await;

    detach_task_runtime(&core, &handle.task_id).await;

    let mut respawn_request = sample_create_request();
    respawn_request.request_id = "req-create-respawn".into();
    respawn_request.resume = Some(resume_state_from_sink(strict.inner()));
    runtime.create_task(respawn_request).await.unwrap();
    runtime
        .submit_input(submit_turn(
            &handle.task_id,
            "req-turn-2-respawn",
            "msg-user-2-respawn",
            "work-2",
            "turn-2",
            "second after respawn",
            2,
        ))
        .await
        .expect("respawned follow-up submit");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    let records = collect_ordered_records(strict.inner());
    let assistant_1 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::AssistantMessage { message_id, .. }
                    if message_id.contains("step_1")
            )
        })
        .expect("first assistant");
    let user_2 = records
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::UserMessage { message_id, .. } if message_id == "msg-user-2-respawn"
            )
        })
        .expect("respawned user");
    assert!(
        !records.iter().any(|record| {
            matches!(record, PersistRecord::TaskEvent { kind: "idle", .. })
                && task_seq(record) > task_seq(assistant_1)
                && task_seq(record) < task_seq(user_2)
        }),
        "respawn after idle rejection must not restore idle before follow-up; records={records:?}"
    );
}

#[tokio::test]
async fn respawn_continues_assistant_step_message_ids() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("second after restart").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "first").await;
    wait_for_idle(&sink, &handle.task_id).await;
    // Idle is followed by work Succeeded; wait so resume last_task_seq is complete.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    detach_task_runtime(&core, &handle.task_id).await;

    let mut respawn_request = sample_create_request();
    respawn_request.request_id = "req-create-respawn-steps".into();
    respawn_request.resume = Some(resume_state_from_sink(&sink));
    runtime.create_task(respawn_request).await.unwrap();

    run_single_turn(&runtime, &handle.task_id, 2, "second").await;
    wait_for_idle(&sink, &handle.task_id).await;

    let assistant_ids: Vec<String> = sink
        .messages()
        .iter()
        .filter(|commit| matches!(commit.message, piko_protocol::Message::Assistant { .. }))
        .map(|commit| commit.message_id.clone())
        .collect();
    assert!(
        assistant_ids
            .iter()
            .any(|id| id.contains(":step_1:assistant")),
        "first turn must commit step_1; ids={assistant_ids:?}"
    );
    assert!(
        assistant_ids
            .iter()
            .any(|id| id.contains(":step_2:assistant")),
        "respawned turn must continue at step_2, not reuse step_1; ids={assistant_ids:?}"
    );
    assert_eq!(
        assistant_ids
            .iter()
            .filter(|id| id.contains(":step_1:assistant"))
            .count(),
        1,
        "step_1 must not be rewritten after respawn; ids={assistant_ids:?}"
    );
}

#[tokio::test]
async fn gated_idle_sink_allows_prior_idles_then_blocks() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("reply-1").await;
    faux.push_text("reply-2").await;
    faux.push_text("reply-3").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let strict = Arc::new(StrictSeqPersistSink::new());
    let gated = Arc::new(GatedIdlePersistSink::allowing(
        strict.clone() as Arc<dyn PersistSink>,
        2,
    ));
    core.set_persist_sink(gated.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "turn-1").await;
    run_single_turn(&runtime, &handle.task_id, 2, "turn-2").await;

    runtime
        .submit_input(submit_turn(
            &handle.task_id,
            "req-turn-3",
            "msg-user-3",
            "work-3",
            "turn-3",
            "turn-3",
            3,
        ))
        .await
        .expect("third turn submit");
    gated.wait_until_idle_blocked().await;

    let records = collect_ordered_records(strict.inner());
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, PersistRecord::TaskEvent { kind: "idle", .. }))
            .count(),
        2,
        "first two idles must pass the gate; records={records:?}"
    );
    gated.release();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let records_after = collect_ordered_records(strict.inner());
    assert!(
        records_after
            .iter()
            .filter(|record| matches!(record, PersistRecord::TaskEvent { kind: "idle", .. }))
            .count()
            >= 3,
        "releasing the gate must allow the blocked idle; records={records_after:?}"
    );
}

#[tokio::test]
async fn respawn_while_prior_idle_gated_cancels_old_runtime_before_follow_up() {
    let faux = Arc::new(FauxProvider::new());
    for turn in 1..=4 {
        faux.push_text(format!("reply-{turn}")).await;
    }
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let strict = Arc::new(StrictSeqPersistSink::new());
    let gated = Arc::new(GatedIdlePersistSink::allowing(
        strict.clone() as Arc<dyn PersistSink>,
        2,
    ));
    core.set_persist_sink(gated.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("main")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    run_single_turn(&runtime, &handle.task_id, 1, "turn-1").await;
    run_single_turn(&runtime, &handle.task_id, 2, "turn-2").await;

    runtime
        .submit_input(submit_turn(
            &handle.task_id,
            "req-turn-3",
            "msg-user-3",
            "work-3",
            "turn-3",
            "turn-3",
            3,
        ))
        .await
        .expect("third turn submit before idle gate");
    gated.wait_until_idle_blocked().await;

    detach_task_runtime(&core, &handle.task_id).await;

    let mut respawn_request = sample_create_request();
    respawn_request.request_id = "req-create-respawn".into();
    respawn_request.resume = Some(resume_state_from_sink(strict.inner()));
    runtime.create_task(respawn_request).await.unwrap();
    runtime
        .submit_input(submit_turn(
            &handle.task_id,
            "req-turn-4",
            "msg-user-4",
            "work-4",
            "turn-4",
            "turn-4",
            4,
        ))
        .await
        .expect("turn-4 submit after respawn");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records_before_release = collect_ordered_records(strict.inner());
    let user_4 = records_before_release
        .iter()
        .find(|record| {
            matches!(
                record,
                PersistRecord::UserMessage { message_id, .. } if message_id == "msg-user-4"
            )
        })
        .expect("turn-4 user before idle release");
    assert!(
        task_seq(user_4) > 0,
        "respawned runtime must accept follow-up after canceling the old handle; records={records_before_release:?}"
    );

    gated.release();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    // Old runtime was cancelled while blocked on idle; it must not mark work-3
    // Succeeded after the replacement runtime already moved on.
    let work_3_succeeded = strict.inner().work_events().iter().any(|event| {
        event.snapshot.work_id == "work-3"
            && event.snapshot.status == piko_protocol::agent_runtime::WorkStatus::Succeeded
    });
    assert!(
        !work_3_succeeded,
        "cancelled prior runtime must not complete work-3 after respawn; records={:?}",
        collect_ordered_records(strict.inner())
    );
}
