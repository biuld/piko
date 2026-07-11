use crate::domain::events::event::Event;
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::dispatch::consumer::DispatchIdentity;
use crate::runtime::events::{SharedSessionOutputHub, TaskEventEmitter};
use crate::runtime::utils::now_ms;
use piko_protocol::TaskEvent;

pub(crate) struct TaskLifecycleConsumer {
    emitter: TaskEventEmitter,
}

impl TaskLifecycleConsumer {
    pub(crate) fn new(
        senders: Option<DispatchSenders>,
        output_hub: Option<SharedSessionOutputHub>,
        identity: DispatchIdentity,
        turn_id: String,
        task_seq: u64,
    ) -> Self {
        Self {
            emitter: TaskEventEmitter::new(identity, turn_id, output_hub, senders, task_seq),
        }
    }

    pub(crate) fn take_events(&self) -> Vec<Event> {
        self.emitter.take_local_events()
    }

    async fn emit_from_context<F>(&self, build: F)
    where
        F: FnOnce(&str, &str, &str, &str) -> TaskEvent,
    {
        let identity = self.emitter.identity.clone();
        let event = build(
            identity.session_id(),
            &self.emitter.turn_id,
            identity.task_id(),
            identity.agent_id(),
        );
        self.emitter.emit_task_lifecycle(event).await;
    }

    pub(crate) async fn on_task_created(
        &self,
        parent_task_id: Option<&str>,
        source_agent_id: Option<&str>,
        prompt: &str,
        turn_id: &str,
    ) {
        let identity = self.emitter.identity.clone();
        let event = TaskEvent::Created {
            session_id: identity.session_id().to_string(),
            turn_id: turn_id.to_string(),
            task_id: identity.task_id().to_string(),
            agent_id: identity.agent_id().to_string(),
            parent_task_id: parent_task_id.map(str::to_string),
            source_agent_id: source_agent_id.map(str::to_string),
            prompt: prompt.to_string(),
            timestamp: now_ms(),
        };
        self.emitter.emit_task_lifecycle(event).await;
    }

    pub(crate) async fn on_task_started(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Started {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_steered(
        &self,
        source_task_id: &str,
        source_agent_id: &str,
        message: &str,
    ) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, _agent_id| TaskEvent::Steered {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                source_task_id: source_task_id.to_string(),
                source_agent_id: source_agent_id.to_string(),
                message: message.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_idle(&self, total_steps: u32, summary: &str) {
        self.emit_from_context(|session_id, _turn_id, task_id, agent_id| TaskEvent::Idle {
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            total_steps,
            summary: summary.to_string(),
            timestamp: now_ms(),
        })
        .await;
    }

    pub(crate) async fn on_task_failed(&self, error: &str) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Failed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                error: error.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_completed(&self, total_steps: u32, summary: &str) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Completed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                total_steps,
                summary: summary.to_string(),
                final_status: "completed".into(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_cancelled(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Cancelled {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_closed(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Closed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_reopened(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Reopened {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }
}
