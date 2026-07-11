use std::collections::HashSet;
use tokio::sync::mpsc;

use crate::domain::events::event::Event;
use crate::domain::model::transcript::{Message, TranscriptManager};
use crate::domain::tasks::task::AgentTask;
use crate::runtime::dispatch::step::{CompletedStep, LocalStepOutput};
use crate::runtime::types::{TaskInputEnvelope, TaskMailboxMessage};
use crate::runtime::utils::now_ms;

use super::step::{AppliedStep, StepCycle};

pub(super) struct TaskRunState {
    senders: Option<crate::runtime::dispatch::DispatchSenders>,
    transcript: TranscriptManager,
    allow_followup_turns: bool,
    pub(super) control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    closed: bool,
    pending_wait_summary: Option<String>,
    step_count: u32,
    last_task_seq: u64,
    head_message_id: Option<String>,
    committed_message_ids: HashSet<String>,
}

impl TaskRunState {
    pub(super) fn new(
        task: &AgentTask,
        control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
        allow_followup_turns: bool,
    ) -> Self {
        let transcript = TranscriptManager::new(task.history.clone());

        Self {
            senders,
            transcript,
            allow_followup_turns,
            control_rx,
            closed: false,
            pending_wait_summary: None,
            step_count: 0,
            last_task_seq: 0,
            head_message_id: None,
            committed_message_ids: HashSet::new(),
        }
    }

    pub(super) fn next_task_seq(&mut self) -> u64 {
        self.last_task_seq += 1;
        self.last_task_seq
    }

    pub(super) fn head_message_id(&self) -> Option<String> {
        self.head_message_id.clone()
    }

    pub(super) fn record_head(&mut self, message_id: String, task_seq: u64) {
        self.head_message_id = Some(message_id.clone());
        self.committed_message_ids.insert(message_id);
        self.last_task_seq = task_seq;
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

    pub(super) fn senders(&self) -> Option<&crate::runtime::dispatch::DispatchSenders> {
        self.senders.as_ref()
    }

    pub(super) fn senders_owned(&self) -> Option<crate::runtime::dispatch::DispatchSenders> {
        self.senders.clone()
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

    pub(super) fn deactivate_channels(&mut self) {
        self.senders = None;
    }

    pub(super) fn activate_channels(
        &mut self,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) {
        if let Some(senders) = senders {
            self.senders = Some(senders);
        }
    }

    pub(super) fn push_user_message(&mut self, message: String) {
        self.transcript.push_user(message);
    }

    pub(super) fn accept_input(&mut self, envelope: &TaskInputEnvelope) {
        self.pending_wait_summary = None;
        self.activate_channels(envelope.senders.clone());
    }

    pub(super) fn drain_controls(&mut self) -> Vec<TaskMailboxMessage> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.control_rx.try_recv() {
            messages.push(msg);
        }
        messages
    }

    pub(super) fn collect_local_step_events(
        &self,
        display_events: Vec<crate::runtime::dispatch::DisplayEvent>,
        persist_events: Vec<crate::runtime::dispatch::PersistEvent>,
    ) -> Vec<Event> {
        if self.senders().is_some() {
            return Vec::new();
        }

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
        let crate::runtime::dispatch::StepDispatchResult { step, local_output } = result;
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
