use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

use crate::domain::events::event::Event;
use crate::domain::model::transcript::{Message, TranscriptManager};
use crate::domain::tasks::task::AgentTask;
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::events::{SharedSessionOutputHub, TaskEventEmitter};
use crate::runtime::step::{CompletedStep, LocalStepOutput};
use crate::runtime::types::{TaskInputEnvelope, TaskMailboxMessage};
use crate::runtime::utils::now_ms;

use super::step::{AppliedStep, StepCycle};

pub(super) struct TaskRunState {
    output_hub: Option<SharedSessionOutputHub>,
    persist_sink: Option<Arc<dyn crate::integration::PersistSink>>,
    head_message_id: Arc<Mutex<Option<String>>>,
    task_seq: Arc<AtomicU64>,
    transcript: TranscriptManager,
    allow_followup_turns: bool,
    pub(super) control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    stashed_controls: Vec<TaskMailboxMessage>,
    closed: bool,
    pending_wait_summary: Option<String>,
    step_count: u32,
    last_task_seq: u64,
    committed_message_ids: HashSet<String>,
    active_work_id: Option<String>,
    active_source_turn_id: Option<String>,
}

impl TaskRunState {
    pub(super) fn new(
        task: &AgentTask,
        control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
        output_hub: Option<SharedSessionOutputHub>,
        persist_sink: Option<Arc<dyn crate::integration::PersistSink>>,
        allow_followup_turns: bool,
    ) -> Self {
        let transcript = TranscriptManager::new(task.history.clone());

        Self {
            output_hub,
            persist_sink,
            head_message_id: Arc::new(Mutex::new(None)),
            task_seq: Arc::new(AtomicU64::new(0)),
            transcript,
            allow_followup_turns,
            control_rx,
            stashed_controls: Vec::new(),
            closed: false,
            pending_wait_summary: None,
            step_count: 0,
            last_task_seq: 0,
            committed_message_ids: HashSet::new(),
            active_work_id: None,
            active_source_turn_id: None,
        }
    }

    pub(super) fn next_task_seq(&mut self) -> u64 {
        self.last_task_seq += 1;
        self.task_seq.store(self.last_task_seq, Ordering::Relaxed);
        self.last_task_seq
    }

    pub(super) fn head_message_id(&self) -> Option<String> {
        self.head_message_id
            .lock()
            .expect("head lock poisoned")
            .clone()
    }

    pub(super) fn record_head(&mut self, message_id: String, task_seq: u64) {
        *self.head_message_id.lock().expect("head lock poisoned") = Some(message_id.clone());
        self.committed_message_ids.insert(message_id);
        self.last_task_seq = task_seq;
        self.task_seq.store(task_seq, Ordering::Relaxed);
    }

    pub(super) fn is_message_committed(&self, message_id: &str) -> bool {
        self.committed_message_ids.contains(message_id)
    }

    pub(super) fn has_user_transcript(&self) -> bool {
        self.transcript.to_vec().iter().any(|message| {
            matches!(
                message,
                crate::domain::model::transcript::Message::User { .. }
            )
        })
    }

    pub(super) fn event_emitter(
        &self,
        identity: DispatchIdentity,
        work_id: String,
    ) -> TaskEventEmitter {
        TaskEventEmitter::new(
            identity,
            work_id,
            self.output_hub.clone(),
            self.persist_sink.clone(),
            Arc::clone(&self.head_message_id),
            Arc::clone(&self.task_seq),
        )
    }

    pub(super) fn transcript(&self) -> &TranscriptManager {
        &self.transcript
    }

    pub(super) fn transcript_mut(&mut self) -> &mut TranscriptManager {
        &mut self.transcript
    }

    pub(super) fn step_count(&self) -> u32 {
        self.step_count
    }

    pub(super) fn begin_step(&mut self) -> u32 {
        self.step_count += 1;
        self.step_count
    }

    pub(super) fn can_follow_up(&self) -> bool {
        self.allow_followup_turns
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed
    }

    pub(super) fn close(&mut self) {
        self.closed = true;
    }

    pub(super) fn reopen(&mut self) {
        self.closed = false;
    }

    pub(super) fn wait_for_next_turn(&mut self, summary: String) {
        self.pending_wait_summary = Some(summary);
    }

    pub(super) fn take_pending_wait_summary(&mut self) -> Option<String> {
        self.pending_wait_summary.take()
    }

    pub(super) fn is_waiting_for_next_turn(&self) -> bool {
        self.pending_wait_summary.is_some()
    }

    pub(super) fn push_user_message(&mut self, message: String) {
        self.transcript.push_user(message);
    }

    pub(super) fn active_work_id(&self) -> Option<&str> {
        self.active_work_id.as_deref()
    }

    pub(super) fn active_source_turn_id(&self) -> Option<&str> {
        self.active_source_turn_id.as_deref()
    }

    pub(super) fn event_emitter_for_active_work(
        &self,
        identity: DispatchIdentity,
    ) -> TaskEventEmitter {
        let work_id = self
            .active_work_id
            .clone()
            .unwrap_or_else(|| "work_unknown".to_string());
        self.event_emitter(identity, work_id)
    }

    pub(super) fn accept_input(&mut self, envelope: &TaskInputEnvelope) {
        self.pending_wait_summary = None;
        self.active_work_id = Some(envelope.input.work_id.clone());
        self.active_source_turn_id = envelope.input.source_turn_id.clone();
    }

    pub(super) fn stash_pending_controls(&mut self) {
        while let Ok(msg) = self.control_rx.try_recv() {
            self.stashed_controls.push(msg);
        }
    }

    pub(super) fn has_stashed_controls(&self) -> bool {
        !self.stashed_controls.is_empty()
    }

    pub(super) fn drain_controls(&mut self) -> Vec<TaskMailboxMessage> {
        self.stash_pending_controls();
        std::mem::take(&mut self.stashed_controls)
    }

    pub(super) fn collect_local_step_events(
        &self,
        display_events: Vec<piko_protocol::DisplayEvent>,
        persist_events: Vec<piko_protocol::PersistEvent>,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        for display_event in display_events {
            events.push(Event::Display(display_event));
        }
        for persist_event in persist_events {
            events.push(Event::Persist(persist_event));
        }
        events
    }

    pub(super) fn apply_step_result(&mut self, cycle: StepCycle) -> AppliedStep {
        let StepCycle {
            result,
            routes,
            model,
            message_id,
        } = cycle;
        let crate::runtime::step::StepDispatchResult { step, local_output } = result;
        let CompletedStep {
            assistant_message,
            tool_calls,
        } = step;
        let LocalStepOutput { display, persist } = local_output;

        let events = self.collect_local_step_events(display, persist);
        self.transcript_mut()
            .push_assistant(assistant_message.clone());
        for tc in &tool_calls {
            self.transcript_mut().push_message(Message::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                model: Some(model.id.clone()),
                provider: Some(model.provider.clone()),
                timestamp: Some(now_ms()),
            });
        }

        AppliedStep {
            assistant_message,
            tool_calls,
            routes,
            message_id,
            events,
        }
    }
}
