use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_CHILD_RUN, TRAJECTORY_EVENT_MODEL_STEP,
    TRAJECTORY_EVENT_SYSTEM_NOTIFICATION, TRAJECTORY_EVENT_TERMINAL, TRAJECTORY_EVENT_TOOL_CALL,
    TrajectoryAssemblyRecord, TrajectoryChildRunRecord, TrajectoryMessage,
    TrajectoryModelStepRecord, TrajectoryRecord, TrajectorySystemNotificationRecord,
    TrajectoryTerminalKind, TrajectoryTerminalRecord, TrajectoryToolCallRecord,
};
use serde::{Deserialize, Serialize};

use super::files::atomic_json;
use crate::Result;
use crate::journal::DurableCommit;
use crate::schema::EventData;

use super::files::{self, READ_MODEL_SCHEMA};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryProjection {
    pub revision: u64,
    pub runs: BTreeMap<String, TrajectoryRunProjection>,
    #[serde(default)]
    pub execution_to_run: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRunProjection {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryFile {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub through_revision: u64,
    pub through_checksum: String,
    pub projection: TrajectoryProjection,
}

pub(crate) fn write(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    projection: &TrajectoryProjection,
    checksum: &str,
) -> Result<()> {
    atomic_json(
        &files::trajectory_path(path),
        &TrajectoryFile {
            schema_version: READ_MODEL_SCHEMA,
            session_id: session_id.to_string(),
            journal_generation: journal_generation.to_string(),
            through_revision: projection.revision,
            through_checksum: checksum.to_string(),
            projection: projection.clone(),
        },
    )
}

pub(crate) fn load(path: &Path) -> Result<Option<TrajectoryFile>> {
    files::load_json(&files::trajectory_path(path))
}

pub fn apply_commit(decoded: &mut TrajectoryProjection, commit: &DurableCommit) {
    for event in &commit.events {
        apply_event(decoded, &event.event_type, &event.payload);
    }
    decoded.revision = decoded.revision.max(commit.revision);
}

fn apply_event(decoded: &mut TrajectoryProjection, event_type: &str, payload: &serde_json::Value) {
    match event_type {
        "execution_started" => {
            let Ok(EventData::ExecutionStarted(started)) =
                serde_json::from_value::<EventData>(payload.clone())
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
            }) = serde_json::from_value::<EventData>(payload.clone())
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
                serde_json::from_value::<EventData>(payload.clone())
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
            let Ok(record) = serde_json::from_value::<TrajectoryAssemblyRecord>(payload.clone())
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
            let Ok(record) = serde_json::from_value::<TrajectoryModelStepRecord>(payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.run_id.clone())
                .or_default();
            run.step_count += 1;
            run.records
                .push(TrajectoryRecord::ModelStep(Box::new(record)));
        }
        TRAJECTORY_EVENT_TOOL_CALL => {
            let Ok(record) = serde_json::from_value::<TrajectoryToolCallRecord>(payload.clone())
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
            let Ok(record) = serde_json::from_value::<TrajectoryChildRunRecord>(payload.clone())
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
                serde_json::from_value::<TrajectorySystemNotificationRecord>(payload.clone())
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
            let Ok(record) = serde_json::from_value::<TrajectoryTerminalRecord>(payload.clone())
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
