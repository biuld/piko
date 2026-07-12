use std::collections::{HashMap, VecDeque};

use crate::api::{
    AgentId, AgentTaskState, ContentBlock, Message, MessageContent, ProtocolError, ServerMessage,
    SessionId, SessionSnapshot, SessionSummary, SessionTreeEntry, TurnId, TurnSnapshot, TurnStatus,
};
use piko_protocol::messages::Usage;
use piko_protocol::{AgentViewSnapshot, SequencedServerMessage};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct HostState {
    sessions: HashMap<SessionId, SessionState>,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub entries: Vec<SessionTreeEntry>,
    pub tasks: HashMap<String, AgentTaskState>,
    pub active_turn_id: Option<TurnId>,
    pub name: Option<String>,
    pub current_leaf_id: Option<String>,
    /// Last committed transcript message for each runtime task.
    pub task_heads: HashMap<String, String>,
    /// Queue of pending steering messages: (task_id, message)
    pub steer_queue: Vec<(String, String)>,
    /// Queue of pending follow-up prompts
    pub follow_up_queue: Vec<String>,
    /// Queue of pending next-turn prompts
    pub next_turn_queue: Vec<String>,
    /// Cumulative token usage and cost across all turns in this session
    pub cumulative_usage: Usage,
    /// Tracked task executions from lifecycle events, keyed by task_id.
    pub active_agents: HashMap<String, crate::api::AgentInfo>,
    /// Runtime task the TUI is currently viewing.
    pub active_task_id: Option<String>,
    /// Per-task live view replay state.
    pub agent_views: HashMap<String, AgentViewState>,
    pub next_agent_view_seq: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AgentViewState {
    pub task_id: String,
    pub agent_id: AgentId,
    pub events: VecDeque<SequencedServerMessage>,
    pub next_seq: u64,
}

impl AgentViewState {
    const MAX_REPLAY_EVENTS: usize = 2_000;

    fn new(task_id: String, agent_id: AgentId) -> Self {
        Self {
            task_id,
            agent_id,
            events: VecDeque::new(),
            next_seq: 1,
        }
    }

    fn push(&mut self, event: SequencedServerMessage) {
        self.next_seq = event.seq.saturating_add(1);
        self.events.push_back(event);
        while self.events.len() > Self::MAX_REPLAY_EVENTS {
            self.events.pop_front();
        }
    }

    fn replay_after(&self, after_seq: Option<u64>) -> Vec<SequencedServerMessage> {
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
            tasks: HashMap::new(),
            active_turn_id: None,
            name: None,
            current_leaf_id: None,
            task_heads: HashMap::new(),
            steer_queue: Vec::new(),
            follow_up_queue: Vec::new(),
            next_turn_queue: Vec::new(),
            cumulative_usage: Usage::empty(),
            active_agents: HashMap::new(),
            active_task_id: None,
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

impl HostState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(&mut self, cwd: impl Into<String>) -> crate::api::CommandResult {
        let cwd = cwd.into();
        let state = SessionState::new(format!("session_{}", Uuid::new_v4()), cwd.clone());
        let session_id = state.session_id.clone();
        self.sessions.insert(session_id.clone(), state);
        crate::api::CommandResult::SessionCreated {
            session_id,
            cwd,
            timestamp: now_ms(),
        }
    }

    pub fn insert_session(&mut self, state: SessionState) {
        self.sessions.insert(state.session_id.clone(), state);
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub fn delete_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        let mut sessions = self
            .sessions
            .values()
            .map(|s| s.summary(None, None, None, None))
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        sessions
    }

    pub fn snapshot(&self, session_id: &str) -> Result<SessionSnapshot, ProtocolError> {
        Ok(self.session(session_id)?.snapshot())
    }

    pub fn append_entry(
        &mut self,
        session_id: &str,
        entry: SessionTreeEntry,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
        if let SessionTreeEntry::SessionInfo(session_info) = &entry
            && let Some(name) = &session_info.name
        {
            state.name = Some(name.clone());
        }
        state.entries.push(entry);
        state.seq += 1;
        Ok(())
    }

    pub fn append_task_entry(
        &mut self,
        session_id: &str,
        task_id: &str,
        entry: SessionTreeEntry,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if let Some(existing) = state
            .entries
            .iter_mut()
            .find(|current| current.id() == entry.id())
        {
            *existing = entry.clone();
            state
                .task_heads
                .insert(task_id.to_string(), entry.id().to_string());
            return Ok(());
        }
        state
            .task_heads
            .insert(task_id.to_string(), entry.id().to_string());
        if state.active_task_id.as_deref() == Some(task_id) {
            state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
        }
        state.entries.push(entry);
        state.seq += 1;
        Ok(())
    }

    pub fn session_cwd(&self, session_id: &str) -> Result<String, ProtocolError> {
        Ok(self.session(session_id)?.cwd.clone())
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn session(&self, session_id: &str) -> Result<&SessionState, ProtocolError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))
    }

    pub fn session_mut(&mut self, session_id: &str) -> Result<&mut SessionState, ProtocolError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))
    }

    // ---- Turn lifecycle ----

    pub fn start_turn(
        &mut self,
        session_id: &str,
    ) -> Result<(TurnId, Vec<ServerMessage>), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if let Some(active) = state.active_turn_id.as_ref() {
            tracing::info!(
                session_id = %session_id,
                active_turn_id = %active,
                "refusing start_turn while a turn is still active"
            );
            return Err(ProtocolError::ActiveTurnExists(session_id.to_string()));
        }
        let turn_id = format!("turn_{}", Uuid::new_v4());
        state.active_turn_id = Some(turn_id.clone());
        Ok((turn_id, Vec::new()))
    }

    pub fn complete_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Completed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                total_tasks: 1,
                timestamp: now_ms(),
            },
        ))
    }

    pub fn fail_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        error: impl Into<String>,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Failed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                error: error.into(),
                timestamp: now_ms(),
            },
        ))
    }

    pub fn cancel_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Cancelled {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                timestamp: now_ms(),
            },
        ))
    }

    pub fn clear_active_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(())
    }

    // ---- Queue management ----

    /// Push a steering message into the session's steer queue.
    pub fn push_steer(
        &mut self,
        session_id: &str,
        task_id: &str,
        message: &str,
    ) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state
            .steer_queue
            .push((task_id.to_string(), message.to_string()));
        self.build_queue_update(session_id)
    }

    /// Push a follow-up prompt into the session's follow-up queue.
    pub fn push_follow_up(&mut self, session_id: &str, message: &str) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state.follow_up_queue.push(message.to_string());
        self.build_queue_update(session_id)
    }

    /// Push a next-turn prompt into the session's next-turn queue.
    pub fn push_next_turn(&mut self, session_id: &str, message: &str) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state.next_turn_queue.push(message.to_string());
        self.build_queue_update(session_id)
    }

    /// Pop and return the next follow-up prompt, if any.
    pub fn drain_next_follow_up(&mut self, session_id: &str) -> Option<String> {
        let state = self.session_mut(session_id).ok()?;
        if state.follow_up_queue.is_empty() {
            None
        } else {
            Some(state.follow_up_queue.remove(0))
        }
    }

    /// Pop and return the next next-turn prompt, if any.
    pub fn drain_next_next_turn(&mut self, session_id: &str) -> Option<String> {
        let state = self.session_mut(session_id).ok()?;
        if state.next_turn_queue.is_empty() {
            None
        } else {
            Some(state.next_turn_queue.remove(0))
        }
    }

    /// Pop and return the next steer item (task_id, message), if any.
    pub fn drain_next_steer(&mut self, session_id: &str) -> Option<(String, String)> {
        let state = self.session_mut(session_id).ok()?;
        if state.steer_queue.is_empty() {
            None
        } else {
            Some(state.steer_queue.remove(0))
        }
    }

    /// Build a QueueUpdate ServerMessage for the given session.
    pub fn build_queue_update(&self, session_id: &str) -> QueueUpdateEvent {
        if let Some(state) = self.sessions.get(session_id) {
            QueueUpdateEvent {
                session_id: session_id.to_string(),
                steer_count: state.steer_queue.len() as u32,
                follow_up_count: state.follow_up_queue.len() as u32,
                next_turn_count: state.next_turn_queue.len() as u32,
                steer_preview: state.steer_queue.last().map(|(_, m)| truncate_preview(m)),
                follow_up_preview: state.follow_up_queue.last().map(|m| truncate_preview(m)),
            }
        } else {
            QueueUpdateEvent::default()
        }
    }
}

/// Intermediate type for building QueueUpdate. Converted to ServerMessage by caller.
#[derive(Debug, Clone, Default)]
pub struct QueueUpdateEvent {
    pub session_id: String,
    pub steer_count: u32,
    pub follow_up_count: u32,
    pub next_turn_count: u32,
    pub steer_preview: Option<String>,
    pub follow_up_preview: Option<String>,
}

impl From<QueueUpdateEvent> for ServerMessage {
    fn from(q: QueueUpdateEvent) -> Self {
        ServerMessage::Queue(crate::api::QueueEvent::Updated {
            session_id: q.session_id,
            steer_count: q.steer_count,
            follow_up_count: q.follow_up_count,
            next_turn_count: q.next_turn_count,
            steer_preview: q.steer_preview,
            follow_up_preview: q.follow_up_preview,
        })
    }
}

impl HostState {
    pub fn get_agent_list(&self, session_id: &str) -> Vec<crate::api::AgentInfo> {
        if let Ok(state) = self.session(session_id) {
            let mut agents = state.active_agents.values().cloned().collect::<Vec<_>>();
            agents.sort_by(|a, b| {
                let a_depth = agent_task_depth(&state.active_agents, &a.task_id);
                let b_depth = agent_task_depth(&state.active_agents, &b.task_id);
                a_depth
                    .cmp(&b_depth)
                    .then_with(|| a.task_id.cmp(&b.task_id))
            });
            agents
        } else {
            vec![]
        }
    }

    pub fn set_active_task(
        &mut self,
        session_id: &str,
        task_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        state.active_task_id = Some(task_id.to_string());
        Ok(())
    }

    pub fn append_agent_view_event(
        &mut self,
        session_id: &str,
        task_id: &str,
        agent_id: &str,
        message: ServerMessage,
    ) -> Result<u64, ProtocolError> {
        let state = self.session_mut(session_id)?;
        let seq = state.next_agent_view_seq;
        state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
        let view = state
            .agent_views
            .entry(task_id.to_string())
            .or_insert_with(|| AgentViewState::new(task_id.to_string(), agent_id.to_string()));
        view.push(SequencedServerMessage {
            seq,
            message: Box::new(message),
        });
        Ok(seq)
    }

    pub fn agent_view_snapshot(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> Result<AgentViewSnapshot, ProtocolError> {
        let state = self.session(session_id)?;
        let view = state
            .agent_views
            .get(task_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("unknown task {task_id}")))?;
        let info = state.active_agents.get(task_id);
        Ok(AgentViewSnapshot {
            task_id: view.task_id.clone(),
            agent_id: view.agent_id.clone(),
            parent_task_id: info.and_then(|info| info.parent_task_id.clone()),
            status: info.map(|info| info.status.clone()),
            next_seq: view.next_seq,
            events: view.events.iter().cloned().collect(),
        })
    }

    pub fn agent_view_replay(
        &self,
        session_id: &str,
        task_id: &str,
        after_seq: Option<u64>,
    ) -> Result<Vec<SequencedServerMessage>, ProtocolError> {
        let state = self.session(session_id)?;
        let view = state
            .agent_views
            .get(task_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("unknown task {task_id}")))?;
        Ok(view.replay_after(after_seq))
    }
}

fn agent_task_depth(agents: &HashMap<String, crate::api::AgentInfo>, task_id: &str) -> usize {
    let mut depth = 0;
    let mut current = agents
        .get(task_id)
        .and_then(|agent| agent.parent_task_id.as_ref());
    while let Some(parent_id) = current {
        depth += 1;
        current = agents
            .get(parent_id)
            .and_then(|agent| agent.parent_task_id.as_ref());
    }
    depth
}

fn truncate_preview(text: &str) -> String {
    if text.len() <= 80 {
        return text.to_string();
    }
    let mut end = 77.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

impl SessionState {
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            entries: self.entries.clone(),
            tasks: self.tasks.clone(),
            current_leaf_id: self.current_leaf_id.clone(),
            active_turn: self.active_turn_id.as_ref().map(|turn_id| TurnSnapshot {
                turn_id: turn_id.clone(),
                status: TurnStatus::Running,
                assistant_text: String::new(),
                tool_calls: Vec::new(),
            }),
            pending_approvals: Vec::new(),
            pending_interactions: Vec::new(),
            name: self.name.clone(),
            cumulative_usage: Some(self.cumulative_usage.clone()),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::RealtimeMessageEvent;
    use piko_protocol::agent_runtime::RealtimeDelta;

    fn realtime(task_id: &str, agent_id: &str, message_id: &str, seq: u64) -> ServerMessage {
        ServerMessage::RealtimeMessage(RealtimeMessageEvent {
            session_id: "session".into(),
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            message_id: message_id.into(),
            delta_seq: seq,
            delta: RealtimeDelta::MessageStarted {
                role: crate::api::MessageRole::Assistant,
            },
        })
    }

    #[test]
    fn agent_view_store_records_task_views_and_replays_by_task() {
        let mut state = HostState::new();
        let session_id = match state.create_session("/tmp") {
            crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
            _ => panic!("expected session created"),
        };

        state
            .append_agent_view_event(&session_id, "t1", "main", realtime("t1", "main", "m1", 0))
            .unwrap();
        state
            .append_agent_view_event(&session_id, "t2", "child", realtime("t2", "child", "m2", 0))
            .unwrap();
        state
            .append_agent_view_event(
                &session_id,
                "t1",
                "main",
                ServerMessage::RealtimeMessage(RealtimeMessageEvent {
                    session_id: "session".into(),
                    task_id: "t1".into(),
                    agent_id: "main".into(),
                    message_id: "m1".into(),
                    delta_seq: 1,
                    delta: RealtimeDelta::Text {
                        content_index: 0,
                        delta: "hello".into(),
                    },
                }),
            )
            .unwrap();
        state
            .append_agent_view_event(&session_id, "t3", "main", realtime("t3", "main", "m3", 0))
            .unwrap();

        let main = state.agent_view_snapshot(&session_id, "t1").unwrap();
        assert_eq!(main.agent_id, "main");
        assert_eq!(main.task_id, "t1");
        assert_eq!(main.events.len(), 2);
        assert_eq!(main.next_seq, 4);

        let replay = state.agent_view_replay(&session_id, "t1", Some(1)).unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, 3);

        let child = state.agent_view_snapshot(&session_id, "t2").unwrap();
        assert_eq!(child.task_id, "t2");
        assert_eq!(child.events.len(), 1);
        assert_eq!(child.events[0].seq, 2);
    }

    #[test]
    fn agent_list_orders_parent_before_child_tasks() {
        let mut state = HostState::new();
        let session_id = match state.create_session("/tmp") {
            crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
            _ => panic!("expected session created"),
        };

        let session = state.session_mut(&session_id).unwrap();
        session.active_agents.insert(
            "task-child".into(),
            crate::api::AgentInfo {
                agent_id: "hello-agent".into(),
                task_id: "task-child".into(),
                parent_task_id: Some("task-main".into()),
                name: "hello-agent".into(),
                role: "assistant".into(),
                status: crate::api::AgentStatus::Running,
            },
        );
        session.active_agents.insert(
            "task-main".into(),
            crate::api::AgentInfo {
                agent_id: "main".into(),
                task_id: "task-main".into(),
                parent_task_id: None,
                name: "main".into(),
                role: "assistant".into(),
                status: crate::api::AgentStatus::Running,
            },
        );

        let agents = state.get_agent_list(&session_id);
        assert_eq!(agents[0].task_id, "task-main");
        assert_eq!(agents[1].task_id, "task-child");
    }
}
