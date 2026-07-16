use std::collections::{HashMap, VecDeque};

use crate::api::{
    AgentId, ContentBlock, Message, MessageContent, SessionId, SessionSummary, SessionTreeEntry,
    TurnId, TurnStatus,
};
use piko_protocol::SequencedServerMessage;
use piko_protocol::messages::Usage;

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
    /// Cumulative token usage and cost across all turns in this session
    pub cumulative_usage: Usage,
    /// Tracked agent instances from lifecycle events, keyed by agent_instance_id.
    pub active_agents: HashMap<String, crate::api::AgentInfo>,
    /// Agent instance the TUI is currently viewing.
    pub active_agent_instance_id: Option<String>,
    /// Per-agent-instance live view replay state.
    pub agent_views: HashMap<String, AgentViewState>,
    pub next_agent_view_seq: u64,
}

#[derive(Debug, Clone)]
pub struct TurnRecord {
    pub turn_id: TurnId,
    pub agent_instance_id: String,
    pub message: String,
    pub status: TurnStatus,
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
            cumulative_usage: Usage::empty(),
            active_agents: HashMap::new(),
            active_agent_instance_id: None,
            agent_views: HashMap::new(),
            next_agent_view_seq: 1,
        }
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
        }
    }

    /// Accumulate usage from an assistant message event.
    pub fn accumulate_usage(&mut self, usage: &Usage) {
        self.cumulative_usage.input += usage.input;
        self.cumulative_usage.output += usage.output;
        self.cumulative_usage.cache_read += usage.cache_read;
        self.cumulative_usage.cache_write += usage.cache_write;
        self.cumulative_usage.total_tokens += usage.total_tokens;
        self.cumulative_usage.cost.input += usage.cost.input;
        self.cumulative_usage.cost.output += usage.cost.output;
        self.cumulative_usage.cost.cache_read += usage.cost.cache_read;
        self.cumulative_usage.cost.cache_write += usage.cost.cache_write;
        self.cumulative_usage.cost.total += usage.cost.total;
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
