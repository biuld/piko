use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

use crate::domain::tasks::task::AgentTask;
use crate::domain::transcript::{Message, TranscriptManager};
use crate::ports::clock::now_ms;
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::events::internal_lifecycle::InternalLifecycleObserver;
use crate::runtime::events::{DeltaSeqState, SharedSessionOutputHub, TaskEventEmitter};
use crate::runtime::persist_sink::SharedPersistSink;
use crate::runtime::step::CompletedStep;
use crate::runtime::task::mailbox::{TaskInputEnvelope, TaskMailboxMessage};

use super::step::{AppliedStep, StepCycle};

pub(super) struct TaskRunState {
    output_hub: SharedSessionOutputHub,
    persist_sink: SharedPersistSink,
    lifecycle_observer: InternalLifecycleObserver,
    head_message_id: Arc<Mutex<Option<String>>>,
    task_seq: Arc<AtomicU64>,
    delta_seq: Arc<Mutex<DeltaSeqState>>,
    persist_error: Arc<Mutex<Option<String>>>,
    persist_commit_lock: Arc<tokio::sync::Mutex<()>>,
    transcript: TranscriptManager,
    allow_followup_turns: bool,
    pub(super) control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    stashed_controls: Vec<TaskMailboxMessage>,
    queued_inputs: VecDeque<TaskInputEnvelope>,
    closed: bool,
    pending_wait_summary: Option<String>,
    step_count: u32,
    committed_message_ids: HashSet<String>,
    active_work_id: Option<String>,
    active_source_turn_id: Option<String>,
}

impl TaskRunState {
    pub(super) fn new(
        task: &AgentTask,
        control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
        output_hub: SharedSessionOutputHub,
        persist_sink: SharedPersistSink,
        lifecycle_observer: InternalLifecycleObserver,
        allow_followup_turns: bool,
    ) -> Self {
        let transcript = TranscriptManager::new(task.history.clone());
        let resume = task.resume.as_ref();
        let head_message_id = resume.and_then(|state| state.head_message_id.clone());
        let last_task_seq = resume.map_or(0, |state| state.last_task_seq);
        let committed_message_ids = resume
            .map(|state| state.committed_message_ids.iter().cloned().collect())
            .unwrap_or_default();
        let step_count = max_assistant_step_count(&committed_message_ids);

        Self {
            output_hub,
            persist_sink,
            lifecycle_observer,
            head_message_id: Arc::new(Mutex::new(head_message_id)),
            task_seq: Arc::new(AtomicU64::new(last_task_seq)),
            delta_seq: Arc::new(Mutex::new(DeltaSeqState::default())),
            persist_error: Arc::new(Mutex::new(None)),
            persist_commit_lock: Arc::new(tokio::sync::Mutex::new(())),
            transcript,
            allow_followup_turns,
            control_rx,
            stashed_controls: Vec::new(),
            queued_inputs: VecDeque::new(),
            closed: false,
            pending_wait_summary: None,
            step_count,
            committed_message_ids,
            active_work_id: None,
            active_source_turn_id: None,
        }
    }

    pub(super) fn next_task_seq(&self) -> u64 {
        self.task_seq.load(Ordering::Relaxed) + 1
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
        self.task_seq.store(task_seq, Ordering::Relaxed);
    }

    pub(super) fn is_message_committed(&self, message_id: &str) -> bool {
        self.committed_message_ids.contains(message_id)
    }

    pub(super) fn has_user_transcript(&self) -> bool {
        self.transcript
            .to_vec()
            .iter()
            .any(|message| matches!(message, crate::domain::transcript::Message::User { .. }))
    }

    pub(super) fn event_emitter(
        &self,
        identity: DispatchIdentity,
        work_id: String,
    ) -> TaskEventEmitter {
        TaskEventEmitter::new(
            identity,
            work_id,
            self.active_source_turn_id.clone(),
            self.output_hub.clone(),
            self.persist_sink.clone(),
            self.lifecycle_observer.clone(),
            Arc::clone(&self.head_message_id),
            Arc::clone(&self.task_seq),
            Arc::clone(&self.delta_seq),
            Arc::clone(&self.persist_error),
            Arc::clone(&self.persist_commit_lock),
        )
    }

    pub(super) fn persist_commit_lock(&self) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(&self.persist_commit_lock)
    }

    pub(super) fn take_persist_error(&self) -> Option<String> {
        self.persist_error
            .lock()
            .expect("persist error lock poisoned")
            .take()
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
        // Close the prior work before the runtime may accept another input/work.
        self.active_work_id = None;
        self.active_source_turn_id = None;
        self.pending_wait_summary = Some(summary);
    }

    pub(super) fn take_pending_wait_summary(&mut self) -> Option<String> {
        self.pending_wait_summary.take()
    }

    pub(super) fn is_waiting_for_next_turn(&self) -> bool {
        self.pending_wait_summary.is_some()
    }

    pub(super) fn has_open_work(&self) -> bool {
        self.active_work_id.is_some()
    }

    pub(super) fn can_start_work(&self) -> bool {
        !self.has_open_work()
    }

    pub(super) fn push_user_content(
        &mut self,
        content: piko_protocol::MessageContent,
        timestamp: Option<i64>,
    ) {
        self.transcript.push_user_content(content, timestamp);
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
        debug_assert!(
            self.can_start_work(),
            "refusing to start work while prior work is still open"
        );
        self.pending_wait_summary = None;
        self.active_work_id = Some(envelope.input.work_id.clone());
        self.active_source_turn_id = envelope.input.source_turn_id.clone();
    }

    pub(super) fn queue_input(&mut self, envelope: TaskInputEnvelope) {
        self.queued_inputs.push_back(envelope);
    }

    pub(super) fn pop_queued_input(&mut self) -> Option<TaskInputEnvelope> {
        self.queued_inputs.pop_front()
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

    pub(super) fn apply_step_result(&mut self, cycle: StepCycle) -> AppliedStep {
        let StepCycle {
            result,
            routes,
            model,
            message_id,
        } = cycle;
        let crate::runtime::step::StepDispatchResult {
            step,
            local_output: _,
        } = result;
        let CompletedStep {
            assistant_message,
            tool_calls,
        } = step;
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
        }
    }
}

/// Highest `step_N` already present in committed assistant message IDs.
/// Resume must continue from here; restarting at 0 reuses `…:step_1:assistant` and
/// trips IdempotencyConflict against the prior turn's assistant row.
fn max_assistant_step_count(message_ids: &HashSet<String>) -> u32 {
    message_ids
        .iter()
        .filter_map(|id| parse_assistant_step_count(id))
        .max()
        .unwrap_or(0)
}

fn parse_assistant_step_count(message_id: &str) -> Option<u32> {
    let without_suffix = message_id.strip_suffix(":assistant")?;
    let step_part = without_suffix.rsplit(':').next()?;
    step_part.strip_prefix("step_")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{max_assistant_step_count, parse_assistant_step_count};
    use std::collections::HashSet;

    #[test]
    fn parses_runtime_assistant_step_ids() {
        assert_eq!(
            parse_assistant_step_count("task_abc:step_4:assistant"),
            Some(4)
        );
        assert_eq!(
            parse_assistant_step_count("task_abc:step_4:assistant:tool_call:0"),
            None
        );
        assert_eq!(parse_assistant_step_count("msg_user_uuid"), None);
    }

    #[test]
    fn resume_step_count_continues_from_max_committed() {
        let ids: HashSet<String> = [
            "msg_user".into(),
            "task_abc:step_1:assistant".into(),
            "task_abc:step_3:assistant".into(),
            "task_abc:step_2:assistant".into(),
        ]
        .into_iter()
        .collect();
        assert_eq!(max_assistant_step_count(&ids), 3);
    }
}
