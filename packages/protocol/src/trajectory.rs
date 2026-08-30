//! Durable agent-run trajectory records (F-36).
//!
//! Trajectory records are written to the session journal as optional
//! (`ignorable`) event types with the `trajectory.` prefix. They are durable
//! and replayable like acknowledged facts but never participate in the
//! acknowledged session-state projection. Two sides of one run record:
//! prompt assembly (input side) and the agent trajectory (interaction side).

use serde::{Deserialize, Serialize};

use crate::{AgentInstanceId, ResolvedToolCatalog, SemanticRunPrompt, Usage};

/// Journal event type for the per-run assembly record.
pub const TRAJECTORY_EVENT_ASSEMBLY: &str = "trajectory.assembly";
/// Journal event type for a model-step record.
pub const TRAJECTORY_EVENT_MODEL_STEP: &str = "trajectory.model_step";
/// Journal event type for a tool-call status record.
pub const TRAJECTORY_EVENT_TOOL_CALL: &str = "trajectory.tool_call";
/// Journal event type for a child-run link record.
pub const TRAJECTORY_EVENT_CHILD_RUN: &str = "trajectory.child_run";
/// Journal event type for a system notification record.
pub const TRAJECTORY_EVENT_SYSTEM_NOTIFICATION: &str = "trajectory.system_notification";
/// Journal event type for the terminal-outcome record.
pub const TRAJECTORY_EVENT_TERMINAL: &str = "trajectory.terminal";

/// Run-scoped identity shared by every trajectory record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryIdentity {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    /// Work identity: the root AgentInput these records belong to.
    pub root_input_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
}

/// Prompt assembly (input side): the exact production prompt and tool catalog
/// frozen for one run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryAssemblyRecord {
    pub identity: TrajectoryIdentity,
    pub assembly_version: u32,
    pub prompt_digest: String,
    pub prompt: SemanticRunPrompt,
    pub tool_catalog: ResolvedToolCatalog,
    pub recorded_at: i64,
}

/// One retry attempt inside a model step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRetryAttempt {
    pub attempt: u32,
    pub delay_ms: u64,
    pub error: String,
    pub started_at: i64,
}

/// A stream-to-non-stream or provider fallback taken inside a model step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryFallback {
    pub from_provider: String,
    pub from_model: String,
    pub to_provider: String,
    pub to_model: String,
    pub reason: String,
    pub at: i64,
}

/// One model step (interaction side): request, options, timing, retries, and
/// fallback. Committed responses are replayed from the journal message
/// referenced by `message_id`; uncommitted responses ride inline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryModelStepRecord {
    pub identity: TrajectoryIdentity,
    pub step_id: String,
    pub provider: String,
    pub model: String,
    pub request: serde_json::Value,
    pub options: serde_json::Value,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retries: Vec<TrajectoryRetryAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<TrajectoryFallback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Final provider usage of the step (input/output/cache tokens and cost).
    /// Absent on start records and on steps whose stream was abandoned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Box<Usage>>,
}

/// Tool-call lifecycle status. A call emits one record per transition;
/// arguments are carried on the first (`Started`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryToolCallStatus {
    Started,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

/// One tool-call status transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryToolCallRecord {
    pub identity: TrajectoryIdentity,
    pub call_id: String,
    pub tool_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    pub status: TrajectoryToolCallStatus,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

/// Link from a parent run to a spawned child agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryChildRunRecord {
    pub identity: TrajectoryIdentity,
    pub child_agent_instance_id: AgentInstanceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run_id: Option<String>,
    pub spawned_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// Kinds of runtime notices recorded for a run. Durable facts (compaction,
/// lifecycle, terminal) are replayed from the journal instead of duplicated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryNotificationKind {
    ApprovalRequested,
    ApprovalResolved,
    SteerDelivered,
    RunError,
    ToolDenied,
    ContextWarning,
}

/// A runtime notice that is not itself an acknowledged journal fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectorySystemNotificationRecord {
    pub identity: TrajectoryIdentity,
    pub kind: TrajectoryNotificationKind,
    pub summary: String,
    pub recorded_at: i64,
}

/// Terminal outcome of a run, recorded as the final trajectory record after
/// the durable `execution_finished` fact (F-36 "terminal record"). Its SSE
/// fan-out is what lets a live viewer observe the running → terminal
/// transition: on a clean completion no other trajectory record would follow
/// `execution_finished`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryTerminalRecord {
    pub identity: TrajectoryIdentity,
    pub kind: TrajectoryTerminalKind,
    /// Human-readable reason carried by the outcome (failure error /
    /// cancellation reason). Absent for a clean completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub finished_at: i64,
}

/// One trajectory record, tagged for query responses and the web viewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrajectoryRecord {
    Assembly(TrajectoryAssemblyRecord),
    /// Boxed: the model-step record dwarfs the other variants (clippy
    /// large_enum_variant) and records are passed by reference downstream.
    ModelStep(Box<TrajectoryModelStepRecord>),
    ToolCall(TrajectoryToolCallRecord),
    ChildRun(TrajectoryChildRunRecord),
    SystemNotification(TrajectorySystemNotificationRecord),
    Terminal(TrajectoryTerminalRecord),
}

/// Terminal outcome of a run. The query derives it from the durable
/// `execution_finished` fact; `TrajectoryRecord::Terminal` also records it as
/// the final observational record so live SSE viewers observe the
/// running → terminal transition. Absent means the run is still running or
/// was interrupted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryTerminalKind {
    Completed,
    Failed,
    Cancelled,
}

/// One entry in the run list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRunSummary {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub root_input_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TrajectoryTerminalKind>,
    pub step_count: u32,
    pub tool_call_count: u32,
    pub child_run_count: u32,
    pub message_count: u32,
    pub dropped_records: u32,
    /// Host-owned run-level usage rollup (F-32 bookkeeping authority): the
    /// viewer formats it, it never aggregates records itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TrajectoryRunUsage>,
}

/// One per-currency cost total for a run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryCostTotal {
    pub currency: String,
    pub total: f64,
}

/// Run-level usage rollup computed by hostd from the model-step records.
/// Token sums include the shared frozen prefix once per step (each step's
/// provider-reported input is cumulative), so `input` is a request total, not
/// a de-duplicated context size.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRunUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Summed per currency across all steps.
    pub cost: Vec<TrajectoryCostTotal>,
    /// `cache_read / input`; `None` when no step reported input tokens.
    pub cache_hit_ratio: Option<f64>,
}

/// Bounded page of run summaries, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRunListPage {
    pub runs: Vec<TrajectoryRunSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// One committed transcript message of a run, flattened so the wire shape of
/// `Message` is preserved and a `messageId` is added for joining model-step
/// records to the assistant message they produced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(flatten)]
    pub message: crate::Message,
}

/// One full run record: assembly (input side) + trajectory records and
/// committed messages (interaction side), joined by run identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRun {
    pub summary: TrajectoryRunSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<TrajectoryAssemblyRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<TrajectoryRecord>,
    /// Committed messages of this run, in journal order (bounded by paging).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<TrajectoryMessage>,
}

/// Live fan-out event consumed by the SSE web viewer. Published after a
/// trajectory record is durably appended (`Record`), or when the session's
/// run list changes — a run started (assembly record) or finished (terminal
/// record) — so a viewer following a different run can refresh the strip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrajectoryLiveEvent {
    /// A record for one run was durably appended.
    Record(Box<TrajectoryLiveRecordEvent>),
    /// The session's run list changed; the viewer should re-fetch it.
    RunsChanged {
        session_id: String,
        committed_at: i64,
    },
}

/// One durably appended trajectory record, tagged for the SSE web viewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryLiveRecordEvent {
    pub session_id: String,
    pub root_input_id: String,
    pub revision: u64,
    pub committed_at: i64,
    pub record: TrajectoryRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> TrajectoryIdentity {
        TrajectoryIdentity {
            session_id: "s".into(),
            agent_instance_id: "a".into(),
            root_input_id: "input-r".into(),
            source_turn_id: None,
        }
    }

    #[test]
    fn terminal_record_round_trip() {
        let record = TrajectoryRecord::Terminal(TrajectoryTerminalRecord {
            identity: identity(),
            kind: TrajectoryTerminalKind::Completed,
            reason: None,
            finished_at: 2,
        });
        let json = serde_json::to_value(&record).unwrap();
        let back: TrajectoryRecord = serde_json::from_value(json).unwrap();
        assert_eq!(record, back);
        match back {
            TrajectoryRecord::Terminal(terminal) => {
                assert_eq!(terminal.kind, TrajectoryTerminalKind::Completed);
                assert_eq!(terminal.reason, None);
            }
            other => panic!("expected terminal record, got {other:?}"),
        }
    }

    #[test]
    fn live_events_round_trip() {
        let record = TrajectoryLiveEvent::Record(Box::new(TrajectoryLiveRecordEvent {
            session_id: "s".into(),
            root_input_id: "input-r".into(),
            revision: 7,
            committed_at: 1,
            record: TrajectoryRecord::Terminal(TrajectoryTerminalRecord {
                identity: identity(),
                kind: TrajectoryTerminalKind::Completed,
                reason: None,
                finished_at: 2,
            }),
        }));
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["kind"], "record");
        assert_eq!(json["rootInputId"], "input-r");
        let back: TrajectoryLiveEvent = serde_json::from_value(json).unwrap();
        assert_eq!(record, back);

        let changed = TrajectoryLiveEvent::RunsChanged {
            session_id: "s".into(),
            committed_at: 3,
        };
        let json = serde_json::to_value(&changed).unwrap();
        assert_eq!(json["kind"], "runs_changed");
        let back: TrajectoryLiveEvent = serde_json::from_value(json).unwrap();
        assert_eq!(changed, back);
    }

    #[test]
    fn trajectory_records_round_trip() {
        let record = TrajectoryRecord::ModelStep(Box::new(TrajectoryModelStepRecord {
            identity: identity(),
            step_id: "step-1".into(),
            provider: "test".into(),
            model: "test-model".into(),
            request: serde_json::json!({"input": "hi"}),
            options: serde_json::json!({"delivery": "streaming"}),
            started_at: 1,
            finished_at: Some(2),
            duration_ms: Some(1),
            retries: vec![TrajectoryRetryAttempt {
                attempt: 1,
                delay_ms: 10,
                error: "boom".into(),
                started_at: 1,
            }],
            fallback: Some(TrajectoryFallback {
                from_provider: "a".into(),
                from_model: "m".into(),
                to_provider: "a".into(),
                to_model: "m".into(),
                reason: "streaming fallback".into(),
                at: 1,
            }),
            response: None,
            message_id: None,
            usage: Some(Box::new(Usage {
                input: 120,
                output: 30,
                cache_read: 90,
                cache_write: 30,
                total_tokens: 150,
                units: Default::default(),
                cost: Default::default(),
            })),
        }));
        let json = serde_json::to_value(&record).unwrap();
        let back: TrajectoryRecord = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(record, back);

        // Backward compatibility: journals written before `usage` existed
        // decode with usage = None.
        let mut legacy = json.clone();
        if let Some(object) = legacy.as_object_mut() {
            object.remove("usage");
        }
        let back: TrajectoryRecord = serde_json::from_value(legacy).unwrap();
        match back {
            TrajectoryRecord::ModelStep(step) => assert_eq!(step.usage, None),
            other => panic!("expected model step record, got {other:?}"),
        }
    }
}
