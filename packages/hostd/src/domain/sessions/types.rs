use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::api::{
    AgentId, ContentBlock, Message, MessageContent, SessionId, SessionSummary, SessionTreeEntry,
    TurnId, TurnStatus,
};
use piko_protocol::SequencedServerMessage;
use piko_protocol::messages::Usage;
use serde::{Deserialize, Serialize};

/// The provider + model id that executed the most recent turn of a session.
/// Hostd owns this record: it is the single source of truth for session
/// model continuity (prompt model-switch fragment, durable JSONL marker).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModelRef {
    pub provider: String,
    pub model_id: String,
}

impl SessionModelRef {
    pub fn new(provider: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model_id: model_id.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct HostState {
    pub(super) sessions: HashMap<SessionId, SessionState>,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub entries: Vec<SessionTreeEntry>,
    pub turns: HashMap<TurnId, TurnRecord>,
    pub active_turns: HashMap<String, TurnId>,
    pub name: Option<String>,
    pub current_leaf_id: Option<String>,
    /// Last committed transcript message for each runtime agent instance.
    pub task_heads: HashMap<String, String>,
    /// Queue of pending steering messages: (agent_instance_id, message)
    pub steer_queue: Vec<(String, String)>,
    /// Last provider+model recorded for a turn in this session. Durable via
    /// `SessionProjection.last_model`; drives the prompt model-switch fragment
    /// and the durable JSONL `ModelChange` marker.
    pub last_model: Option<SessionModelRef>,
    /// Last world-state facts recorded for a turn in this session (F-04
    /// slice 2). Durable via `SessionProjection.world_state_baseline`; drives
    /// the full-vs-diff world-state injection decision.
    pub world_state_baseline: Option<crate::domain::prompts::WorldStateFacts>,
    /// Cumulative token usage and cost across all turns in this session
    pub cumulative_usage: Usage,
    /// Incurred usage by AgentInstance. This is populated from journal usage
    /// facts on replay and updated alongside the live session ledger.
    pub agent_usage: BTreeMap<String, Usage>,
    /// Tracked agent instances from lifecycle events, keyed by agent_instance_id.
    pub active_agents: HashMap<String, crate::api::AgentInfo>,
    /// Agent instance the TUI is currently viewing.
    pub active_agent_instance_id: Option<String>,
    /// Per-agent-instance live view replay state.
    pub agent_views: HashMap<String, AgentViewState>,
    pub next_agent_view_seq: u64,
    /// Budget-window compaction state (F-05): pending guard, window counter,
    /// and rearm baseline. Derived on resume from the last checkpoint entry.
    pub compaction: crate::domain::compaction::CompactionState,
    /// Per-agent durable todo lists (F-27). Keyed by agent_instance_id.
    /// Empty lists are omitted (cleared).
    pub todo_lists: HashMap<String, piko_protocol::TodoList>,
    /// Last successful `todo_write` projection (including empty clear) waiting
    /// for the observation path to emit `TodoListUpdated` + durable persist.
    /// Process-local; not part of snapshot.
    pub pending_todo_projection: Option<piko_protocol::TodoList>,
}

#[derive(Debug, Clone)]
pub struct TurnRecord {
    pub turn_id: TurnId,
    pub agent_instance_id: String,
    pub message: String,
    pub status: TurnStatus,
    /// Rolled-up model-step usage for this turn (hostd ledger; F-15/D-29).
    pub usage: Usage,
    /// Net exact-content changes from built-in workspace mutations.
    pub file_changes: Vec<piko_protocol::TurnFileChange>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentViewState {
    pub agent_instance_id: String,
    pub agent_id: AgentId,
    pub events: VecDeque<SequencedServerMessage>,
    pub next_seq: u64,
}

impl AgentViewState {
    const MAX_REPLAY_EVENTS: usize = 2_000;

    pub(super) fn new(agent_instance_id: String, agent_id: AgentId) -> Self {
        Self {
            agent_instance_id,
            agent_id,
            events: VecDeque::new(),
            next_seq: 1,
        }
    }

    pub(super) fn push(&mut self, event: SequencedServerMessage) {
        self.next_seq = event.seq.saturating_add(1);
        self.events.push_back(event);
        while self.events.len() > Self::MAX_REPLAY_EVENTS {
            self.events.pop_front();
        }
    }

    pub(super) fn replay_after(&self, after_seq: Option<u64>) -> Vec<SequencedServerMessage> {
        let Some(after_seq) = after_seq else {
            return self.events.iter().cloned().collect();
        };
        self.events
            .iter()
            .filter(|event| event.seq > after_seq)
            .cloned()
            .collect()
    }
}

impl SessionState {
    pub fn new(session_id: SessionId, cwd: String) -> Self {
        Self {
            session_id,
            cwd,
            seq: 0,
            entries: Vec::new(),
            turns: HashMap::new(),
            active_turns: HashMap::new(),
            name: None,
            current_leaf_id: None,
            task_heads: HashMap::new(),
            steer_queue: Vec::new(),
            last_model: None,
            world_state_baseline: None,
            cumulative_usage: Usage::empty(),
            agent_usage: BTreeMap::new(),
            active_agents: HashMap::new(),
            active_agent_instance_id: None,
            agent_views: HashMap::new(),
            next_agent_view_seq: 1,
            compaction: Default::default(),
            todo_lists: HashMap::new(),
            pending_todo_projection: None,
        }
    }

    /// Snapshot projection of all non-empty durable todo lists.
    pub fn todo_lists_for_snapshot(&self) -> Vec<piko_protocol::TodoList> {
        let mut lists: Vec<_> = self.todo_lists.values().cloned().collect();
        lists.sort_by(|a, b| a.agent_instance_id.cmp(&b.agent_instance_id));
        lists
    }

    /// Rebuild per-AgentInstance token/cost buckets from durable message facts.
    pub fn agent_usage_for_snapshot(&self) -> Vec<piko_protocol::AgentUsageSummary> {
        let mut rows = BTreeMap::<String, piko_protocol::AgentUsageSummary>::new();

        for agent in self.active_agents.values() {
            rows.entry(agent.agent_instance_id.clone())
                .or_insert_with(|| piko_protocol::AgentUsageSummary {
                    agent_instance_id: agent.agent_instance_id.clone(),
                    agent_id: agent.agent_id.clone(),
                    run_count: None,
                    active_duration_ms: None,
                    usage: Usage::empty(),
                });
        }

        for (agent_instance_id, usage) in &self.agent_usage {
            let row = rows.entry(agent_instance_id.clone()).or_insert_with(|| {
                piko_protocol::AgentUsageSummary {
                    agent_instance_id: agent_instance_id.clone(),
                    agent_id: self
                        .active_agents
                        .get(agent_instance_id)
                        .map(|agent| agent.agent_id.clone())
                        .unwrap_or_else(|| agent_instance_id.clone()),
                    run_count: None,
                    active_duration_ms: None,
                    usage: Usage::empty(),
                }
            });
            row.usage = usage.clone();
        }

        rows.into_values().collect()
    }

    /// Replace one agent's list (full replace). Empty items clear durable
    /// storage but still queue a pending projection so live clients learn
    /// about the clear.
    pub fn set_todo_list(&mut self, list: piko_protocol::TodoList) -> piko_protocol::TodoList {
        let id = list.agent_instance_id.clone();
        if list.items.is_empty() {
            self.todo_lists.remove(&id);
        } else {
            self.todo_lists.insert(id, list.clone());
        }
        self.pending_todo_projection = Some(list.clone());
        list
    }

    /// Take the pending todo projection (if any) for live emit + durable write.
    pub fn take_pending_todo_projection(&mut self) -> Option<piko_protocol::TodoList> {
        self.pending_todo_projection.take()
    }

    #[allow(clippy::collapsible_if)]
    pub fn first_message(&self) -> Option<String> {
        for entry in &self.entries {
            if let SessionTreeEntry::Message(msg_entry) = entry {
                if let Message::User { content, .. } = &msg_entry.message {
                    match content {
                        MessageContent::String(s) => {
                            if !s.trim().is_empty() {
                                return Some(s.clone());
                            }
                        }
                        MessageContent::Blocks(blocks) => {
                            let mut text = String::new();
                            for b in blocks {
                                if let ContentBlock::Text { text: t } = b {
                                    text.push_str(t);
                                }
                            }
                            if !text.trim().is_empty() {
                                return Some(text);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn message_count(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| matches!(e, SessionTreeEntry::Message(_)))
            .count() as u64
    }

    pub fn summary(
        &self,
        created_at: Option<String>,
        modified_at: Option<String>,
        session_path: Option<String>,
        parent_session_path: Option<String>,
    ) -> SessionSummary {
        let first_msg = self.first_message();
        let msg_count = self.message_count();
        let mod_at = modified_at
            .or_else(|| self.entries.last().map(|e| e.timestamp().to_string()))
            .or_else(|| created_at.clone());
        SessionSummary {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            name: self.name.clone(),
            first_message: first_msg,
            message_count: msg_count,
            created_at,
            modified_at: mod_at,
            session_path,
            parent_session_path,
            integrity_error: None,
        }
    }

    /// Accumulate usage from an assistant message (session roll-up).
    pub fn accumulate_usage(&mut self, usage: &Usage) {
        self.cumulative_usage.accumulate(usage);
    }

    /// Account one model-step usage into the product ledger.
    ///
    /// Always updates session `cumulative_usage`. When `turn_id` matches an
    /// open turn record, also rolls the step into that turn's usage total.
    pub fn account_step_usage(&mut self, turn_id: Option<&str>, usage: &Usage) {
        self.accumulate_usage(usage);
        let Some(turn_id) = turn_id else {
            return;
        };
        if let Some(turn) = self.turns.get_mut(turn_id) {
            self.agent_usage
                .entry(turn.agent_instance_id.clone())
                .or_default()
                .accumulate(usage);
            turn.usage.accumulate(usage);
        }
    }
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub(super) fn truncate_preview(text: &str) -> String {
    if text.len() <= 80 {
        return text.to_string();
    }
    let mut end = 77.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}
