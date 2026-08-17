//! Join journal facts and optional trajectory records into per-run views.

use std::collections::{BTreeMap, HashMap};

use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_CHILD_RUN, TRAJECTORY_EVENT_MODEL_STEP,
    TRAJECTORY_EVENT_SYSTEM_NOTIFICATION, TRAJECTORY_EVENT_TERMINAL, TRAJECTORY_EVENT_TOOL_CALL,
    TrajectoryAssemblyRecord, TrajectoryChildRunRecord, TrajectoryCostTotal, TrajectoryMessage,
    TrajectoryModelStepRecord, TrajectoryRecord, TrajectoryRunSummary, TrajectoryRunUsage,
    TrajectorySystemNotificationRecord, TrajectoryTerminalKind, TrajectoryTerminalRecord,
    TrajectoryToolCallRecord,
};
use piko_session_store::EventData;

use crate::ports::storage_types::RawJournalEventRef;

#[derive(Clone, Default)]
pub(super) struct DecodedSession {
    pub revision: u64,
    pub runs: BTreeMap<String, DecodedRun>,
    execution_to_run: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub(super) struct DecodedRun {
    pub agent_instance_id: Option<String>,
    pub execution_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub terminal: Option<TrajectoryTerminalKind>,
    pub assembly: Option<TrajectoryAssemblyRecord>,
    pub records: Vec<TrajectoryRecord>,
    pub messages: Vec<TrajectoryMessage>,
    pub step_count: u32,
    pub tool_call_count: u32,
    pub child_run_count: u32,
}

pub(super) fn apply_events(decoded: &mut DecodedSession, events: &[RawJournalEventRef]) {
    for raw in events {
        apply_event(decoded, raw);
        decoded.revision = decoded.revision.max(raw.revision);
    }
}

fn apply_event(decoded: &mut DecodedSession, raw: &RawJournalEventRef) {
    match raw.event_type.as_str() {
        "execution_started" => {
            let Ok(EventData::ExecutionStarted(started)) =
                serde_json::from_value::<EventData>(raw.payload.clone())
            else {
                return;
            };
            decoded
                .execution_to_run
                .insert(started.execution_id.clone(), started.run_id.clone());
            let run = decoded.runs.entry(started.run_id).or_default();
            run.agent_instance_id = Some(started.agent_instance_id);
            run.execution_id = Some(started.execution_id);
            run.source_turn_id = started.source_turn_id;
            run.started_at = Some(started.started_at);
        }
        "execution_finished" => {
            let Ok(EventData::ExecutionFinished {
                execution_id,
                report,
                finished_at,
            }) = serde_json::from_value::<EventData>(raw.payload.clone())
            else {
                return;
            };
            let Some(run_id) = decoded.execution_to_run.get(&execution_id).cloned() else {
                return;
            };
            let run = decoded.runs.entry(run_id).or_default();
            run.finished_at = Some(finished_at);
            run.terminal = Some(match report.outcome {
                piko_protocol::ExecutionOutcome::Succeeded { .. } => {
                    TrajectoryTerminalKind::Completed
                }
                piko_protocol::ExecutionOutcome::Failed { .. } => TrajectoryTerminalKind::Failed,
                piko_protocol::ExecutionOutcome::Cancelled { .. } => {
                    TrajectoryTerminalKind::Cancelled
                }
            });
        }
        "message_committed" => {
            let Ok(EventData::MessageCommitted(committed)) =
                serde_json::from_value::<EventData>(raw.payload.clone())
            else {
                return;
            };
            let Some(execution_id) = committed.execution_id else {
                return;
            };
            let Some(run_id) = decoded.execution_to_run.get(&execution_id).cloned() else {
                return;
            };
            decoded
                .runs
                .entry(run_id)
                .or_default()
                .messages
                .push(TrajectoryMessage {
                    message_id: Some(committed.message_id.clone()),
                    message: committed.message,
                });
        }
        TRAJECTORY_EVENT_ASSEMBLY => {
            let Ok(record) =
                serde_json::from_value::<TrajectoryAssemblyRecord>(raw.payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.agent_instance_id
                .get_or_insert_with(|| record.identity.agent_instance_id.to_string());
            run.assembly = Some(record);
        }
        TRAJECTORY_EVENT_MODEL_STEP => {
            let Ok(record) =
                serde_json::from_value::<TrajectoryModelStepRecord>(raw.payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.step_count += 1;
            run.records.push(TrajectoryRecord::ModelStep(record));
        }
        TRAJECTORY_EVENT_TOOL_CALL => {
            let Ok(record) =
                serde_json::from_value::<TrajectoryToolCallRecord>(raw.payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.tool_call_count += 1;
            run.records.push(TrajectoryRecord::ToolCall(record));
        }
        TRAJECTORY_EVENT_CHILD_RUN => {
            let Ok(record) =
                serde_json::from_value::<TrajectoryChildRunRecord>(raw.payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.child_run_count += 1;
            run.records.push(TrajectoryRecord::ChildRun(record));
        }
        TRAJECTORY_EVENT_SYSTEM_NOTIFICATION => {
            let Ok(record) =
                serde_json::from_value::<TrajectorySystemNotificationRecord>(raw.payload.clone())
            else {
                return;
            };
            decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default()
                .records
                .push(TrajectoryRecord::SystemNotification(record));
        }
        TRAJECTORY_EVENT_TERMINAL => {
            let Ok(record) =
                serde_json::from_value::<TrajectoryTerminalRecord>(raw.payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.finished_at = Some(record.finished_at);
            run.terminal = Some(record.kind);
            run.records.push(TrajectoryRecord::Terminal(record));
        }
        _ => {}
    }
}

pub(super) fn summarize(
    session_id: &str,
    run_id: &str,
    run: &DecodedRun,
    dropped: &HashMap<String, u32>,
) -> TrajectoryRunSummary {
    TrajectoryRunSummary {
        session_id: session_id.to_string(),
        agent_instance_id: run.agent_instance_id.clone().unwrap_or_default(),
        run_id: run_id.to_string(),
        execution_id: run.execution_id.clone().unwrap_or_default(),
        source_turn_id: run.source_turn_id.clone(),
        started_at: run.started_at.unwrap_or_default(),
        finished_at: run.finished_at,
        terminal: run.terminal,
        step_count: run.step_count,
        tool_call_count: run.tool_call_count,
        child_run_count: run.child_run_count,
        message_count: run.messages.len() as u32,
        dropped_records: dropped.get(run_id).copied().unwrap_or(0),
        usage: run_usage(&run.records),
    }
}

/// Host-owned run-level usage rollup. Token sums add each step's
/// provider-reported input, which is cumulative over the run's conversation;
/// cost is summed per currency.
pub(super) fn run_usage(records: &[TrajectoryRecord]) -> Option<TrajectoryRunUsage> {
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut costs: BTreeMap<String, f64> = BTreeMap::new();
    let mut saw_usage = false;
    for record in records {
        let TrajectoryRecord::ModelStep(step) = record else {
            continue;
        };
        let Some(usage) = step.usage.as_deref() else {
            continue;
        };
        saw_usage = true;
        input += usage.input;
        output += usage.output;
        cache_read += usage.cache_read;
        cache_write += usage.cache_write;
        for entry in &usage.cost.entries {
            *costs.entry(entry.currency.clone()).or_default() += entry.total;
        }
    }
    if !saw_usage {
        return None;
    }
    Some(TrajectoryRunUsage {
        input,
        output,
        cache_read,
        cache_write,
        cost: costs
            .into_iter()
            .map(|(currency, total)| TrajectoryCostTotal { currency, total })
            .collect(),
        cache_hit_ratio: (input > 0).then(|| cache_read as f64 / input as f64),
    })
}
