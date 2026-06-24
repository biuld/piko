// ---- Protocol: event_store — event sourcing journal ----
//
// OrchSourcingEvent: immutable records of everything that happened.
// apply_event: fold an event into OrchState.
// rebuild_state: replay a slice of events to reconstruct state.

use serde::{Deserialize, Serialize};

use super::agents::{AgentRuntimeState, AgentTaskState};
use super::agents::{
    AgentSpec, AgentStatus, AgentTaskId, AgentTaskResult, AgentTaskStatus, TaskSource,
};
use super::messages::Usage;
use super::state::OrchState;

// ---- Sourcing event ----

/// An immutable record of something that happened in the orchestrator.
///
/// Every state change flows through these events. The full journal can be
/// replayed to reconstruct `OrchState` at any point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchSourcingEvent {
    // ── Agent lifecycle ──
    AgentRegistered {
        agent_id: String,
        spec: AgentSpec,
        timestamp: i64,
    },
    AgentUnregistered {
        agent_id: String,
        timestamp: i64,
    },

    // ── Task lifecycle ──
    TaskCreated {
        task_id: AgentTaskId,
        target_agent_id: String,
        prompt: String,
        source: TaskSource,
        parent_task_id: Option<String>,
        timestamp: i64,
    },
    TaskStarted {
        task_id: AgentTaskId,
        agent_id: String,
        timestamp: i64,
    },
    TaskStepCompleted {
        task_id: AgentTaskId,
        agent_id: String,
        step_index: u32,
        stop_reason: String,
        usage: Usage,
        timestamp: i64,
    },
    TaskToolCalled {
        task_id: AgentTaskId,
        agent_id: String,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        timestamp: i64,
    },
    TaskToolResult {
        task_id: AgentTaskId,
        agent_id: String,
        tool_call_id: String,
        ok: bool,
        output: serde_json::Value,
        timestamp: i64,
    },
    TaskCompleted {
        task_id: AgentTaskId,
        agent_id: String,
        result: AgentTaskResult,
        timestamp: i64,
    },
    TaskFailed {
        task_id: AgentTaskId,
        agent_id: String,
        error: String,
        timestamp: i64,
    },
    TaskCancelled {
        task_id: AgentTaskId,
        agent_id: String,
        reason: Option<String>,
        timestamp: i64,
    },

    // ── Model config ──
    ModelConfigSet {
        model_id: String,
        provider_name: String,
        timestamp: i64,
    },

    // ── Tool set ──
    ToolSetRegistered {
        tool_set: super::tools::ToolSet,
        timestamp: i64,
    },
    ToolSetUnregistered {
        tool_set_id: String,
        timestamp: i64,
    },
}

impl OrchSourcingEvent {
    /// Return a human-readable event type string for assertions / logging.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AgentRegistered { .. } => "agent_registered",
            Self::AgentUnregistered { .. } => "agent_unregistered",
            Self::TaskCreated { .. } => "task_created",
            Self::TaskStarted { .. } => "task_started",
            Self::TaskStepCompleted { .. } => "task_step_completed",
            Self::TaskToolCalled { .. } => "task_tool_called",
            Self::TaskToolResult { .. } => "task_tool_result",
            Self::TaskCompleted { .. } => "task_completed",
            Self::TaskFailed { .. } => "task_failed",
            Self::TaskCancelled { .. } => "task_cancelled",
            Self::ModelConfigSet { .. } => "model_config_set",
            Self::ToolSetRegistered { .. } => "tool_set_registered",
            Self::ToolSetUnregistered { .. } => "tool_set_unregistered",
        }
    }
}

// ---- State reconstruction ----

/// Apply a sourcing event to an OrchState, returning the modified state.
pub fn apply_event(mut state: OrchState, event: &OrchSourcingEvent) -> OrchState {
    match event {
        OrchSourcingEvent::AgentRegistered { agent_id, spec, .. } => {
            state.agents.insert(
                agent_id.clone(),
                AgentRuntimeState {
                    id: agent_id.clone(),
                    spec: spec.clone(),
                    status: AgentStatus::Idle,
                    active_task_id: None,
                    transcript: vec![],
                },
            );
        }
        OrchSourcingEvent::AgentUnregistered { agent_id, .. } => {
            state.agents.remove(agent_id);
        }
        OrchSourcingEvent::TaskCreated {
            task_id,
            target_agent_id,
            prompt,
            source,
            parent_task_id,
            ..
        } => {
            state.tasks.insert(
                task_id.clone(),
                AgentTaskState {
                    id: task_id.clone(),
                    target_agent_id: target_agent_id.clone(),
                    prompt: prompt.clone(),
                    source: source.clone(),
                    status: AgentTaskStatus::Queued,
                    priority: 0,
                    parent_task_id: parent_task_id.clone(),
                    result: None,
                    error: None,
                    plan: None,
                },
            );
        }
        OrchSourcingEvent::TaskStarted {
            task_id, agent_id, ..
        } => {
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.status = AgentTaskStatus::Running;
            }
            if let Some(agent) = state.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Running;
                agent.active_task_id = Some(task_id.clone());
            }
        }
        OrchSourcingEvent::TaskCompleted {
            task_id,
            agent_id,
            result,
            ..
        } => {
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.status = AgentTaskStatus::Completed;
                task.result = Some(result.clone());
            }
            if let Some(agent) = state.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Idle;
                agent.active_task_id = None;
            }
        }
        OrchSourcingEvent::TaskFailed {
            task_id,
            agent_id,
            error,
            ..
        } => {
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.status = AgentTaskStatus::Failed;
                task.error = Some(error.clone());
            }
            if let Some(agent) = state.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Idle;
                agent.active_task_id = None;
            }
        }
        OrchSourcingEvent::TaskCancelled {
            task_id,
            agent_id,
            reason,
            ..
        } => {
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.status = AgentTaskStatus::Cancelled;
                task.error = reason.clone();
            }
            if let Some(agent) = state.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Idle;
                agent.active_task_id = None;
            }
        }
        OrchSourcingEvent::TaskStepCompleted { task_id, .. } => {
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.status = AgentTaskStatus::Running;
            }
        }
        // Non-mutating events
        _ => {}
    }

    state
}

/// Rebuild a full OrchState from a slice of sourcing events.
pub fn rebuild_state(events: &[OrchSourcingEvent]) -> OrchState {
    let mut state = OrchState::new("rebuilt".into());
    for event in events {
        state = apply_event(state, event);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::agents::AgentTaskStatus;

    fn test_spec(id: &str) -> AgentSpec {
        AgentSpec {
            id: id.into(),
            name: id.into(),
            role: "test".into(),
            description: None,
            system_prompt: String::new(),
            model: None,
            tool_set_ids: vec![],
            active_tool_names: None,
        }
    }

    #[test]
    fn test_orch_state_new() {
        let state = OrchState::new("test-run".into());
        assert_eq!(state.run_id, "test-run");
    }

    #[test]
    fn test_apply_event_agent_registered() {
        let mut state = OrchState::new("t".into());
        state = apply_event(
            state,
            &OrchSourcingEvent::AgentRegistered {
                agent_id: "a".into(),
                spec: test_spec("a"),
                timestamp: 0,
            },
        );
        assert_eq!(state.agents.len(), 1);
    }

    #[test]
    fn test_apply_event_task_lifecycle() {
        let mut state = OrchState::new("t".into());
        state = apply_event(
            state,
            &OrchSourcingEvent::AgentRegistered {
                agent_id: "w".into(),
                spec: test_spec("w"),
                timestamp: 0,
            },
        );
        state = apply_event(
            state,
            &OrchSourcingEvent::TaskCreated {
                task_id: "t1".into(),
                target_agent_id: "w".into(),
                prompt: "p".into(),
                source: TaskSource::User,
                parent_task_id: None,
                timestamp: 1,
            },
        );
        state = apply_event(
            state,
            &OrchSourcingEvent::TaskCompleted {
                task_id: "t1".into(),
                agent_id: "w".into(),
                result: AgentTaskResult {
                    summary: "ok".into(),
                    artifacts: None,
                },
                timestamp: 2,
            },
        );
        assert_eq!(state.tasks["t1"].status, AgentTaskStatus::Completed);
    }

    #[test]
    fn test_event_kind() {
        let e = OrchSourcingEvent::TaskCreated {
            task_id: "t".into(),
            target_agent_id: "a".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 0,
        };
        assert_eq!(e.kind(), "task_created");
    }

    #[test]
    fn test_rebuild_state() {
        let events = vec![
            OrchSourcingEvent::AgentRegistered {
                agent_id: "a".into(),
                spec: test_spec("a"),
                timestamp: 0,
            },
            OrchSourcingEvent::TaskCreated {
                task_id: "t1".into(),
                target_agent_id: "a".into(),
                prompt: "p".into(),
                source: TaskSource::User,
                parent_task_id: None,
                timestamp: 1,
            },
        ];
        let state = rebuild_state(&events);
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.tasks.len(), 1);
    }

    #[test]
    fn test_non_mutating_events() {
        let state = OrchState::new("t".into());
        let state = apply_event(
            state,
            &OrchSourcingEvent::TaskToolCalled {
                task_id: "t".into(),
                agent_id: "a".into(),
                tool_call_id: "tc".into(),
                tool_name: "read".into(),
                arguments: serde_json::json!({}),
                timestamp: 0,
            },
        );
        // Non-mutating events leave state unchanged
        assert!(state.agents.is_empty());
    }
}
