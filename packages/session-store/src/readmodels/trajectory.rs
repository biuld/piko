use std::collections::{BTreeMap, HashMap, HashSet};
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
    /// Runs keyed by the root AgentInput of the work.
    pub runs: BTreeMap<String, TrajectoryRunProjection>,
    #[serde(default)]
    pub input_contents: HashMap<String, piko_protocol::MessageContent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRunProjection {
    pub agent_instance_id: Option<String>,
    pub root_input_id: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub terminal: Option<TrajectoryTerminalKind>,
    pub assembly: Option<TrajectoryAssemblyRecord>,
    pub records: Vec<TrajectoryRecord>,
    pub messages: Vec<TrajectoryMessage>,
    /// Number of distinct model steps represented by lifecycle records.
    /// A normal step has both a start and a finish record.
    pub step_count: u32,
    /// Number of distinct tool calls represented by lifecycle records.
    pub tool_call_count: u32,
    pub child_run_count: u32,
}

impl TrajectoryProjection {
    /// Recompute counters when loading a projection written by an older
    /// reducer. Lifecycle records remain in `records`, while the counters
    /// describe logical model steps and tool calls.
    pub(crate) fn refresh_counts(&mut self) {
        for run in self.runs.values_mut() {
            run.refresh_counts();
        }
    }
}

impl TrajectoryRunProjection {
    /// Count one logical model step even though trajectory normally stores a
    /// start and a finish record for it.
    pub fn logical_step_count(&self) -> u32 {
        let ids = self
            .records
            .iter()
            .filter_map(|record| match record {
                TrajectoryRecord::ModelStep(step) => Some(step.step_id.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        ids.len().try_into().unwrap_or(u32::MAX)
    }

    /// Count one logical tool call across all of its lifecycle status records.
    pub fn logical_tool_call_count(&self) -> u32 {
        let ids = self
            .records
            .iter()
            .filter_map(|record| match record {
                TrajectoryRecord::ToolCall(call) => Some(call.call_id.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        ids.len().try_into().unwrap_or(u32::MAX)
    }

    fn refresh_counts(&mut self) {
        self.step_count = self.logical_step_count();
        self.tool_call_count = self.logical_tool_call_count();
    }
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
        "agent_input_processing_started_v1" => {
            let Ok(EventData::AgentInputProcessingStartedV1(started)) =
                serde_json::from_value::<EventData>(payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(started.root_input_id.clone())
                .or_default();
            run.agent_instance_id = Some(started.agent_instance_id);
            run.root_input_id = Some(started.root_input_id);
            run.started_at = Some(started.started_at);
        }
        "agent_input_admitted_v1" => {
            let Ok(EventData::AgentInputAdmittedV1(admitted)) =
                serde_json::from_value::<EventData>(payload.clone())
            else {
                return;
            };
            decoded
                .input_contents
                .insert(admitted.input.input_id, admitted.input.content);
        }
        "agent_input_applied_v1" => {
            let Ok(EventData::AgentInputAppliedV1(applied)) =
                serde_json::from_value::<EventData>(payload.clone())
            else {
                return;
            };
            let Some(content) = decoded.input_contents.get(&applied.input_id).cloned() else {
                return;
            };
            decoded
                .runs
                .entry(applied.root_input_id)
                .or_default()
                .messages
                .push(TrajectoryMessage {
                    message_id: Some(applied.message_id),
                    message: piko_protocol::Message::User {
                        content,
                        timestamp: Some(applied.committed_at),
                    },
                });
        }
        "agent_input_processing_finished_v1" => {
            let Ok(EventData::AgentInputProcessingFinishedV1(finished)) =
                serde_json::from_value::<EventData>(payload.clone())
            else {
                return;
            };
            let run = decoded.runs.entry(finished.root_input_id).or_default();
            run.finished_at = Some(finished.finished_at);
            run.terminal = Some(match finished.report.outcome {
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
            let Some(root_input_id) = committed.root_input_id else {
                return;
            };
            decoded
                .runs
                .entry(root_input_id)
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
                .entry(record.identity.root_input_id.clone())
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
                .entry(record.identity.root_input_id.clone())
                .or_default();
            run.records
                .push(TrajectoryRecord::ModelStep(Box::new(record)));
            run.refresh_counts();
        }
        TRAJECTORY_EVENT_TOOL_CALL => {
            let Ok(record) = serde_json::from_value::<TrajectoryToolCallRecord>(payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.root_input_id.clone())
                .or_default();
            run.records.push(TrajectoryRecord::ToolCall(record));
            run.refresh_counts();
        }
        TRAJECTORY_EVENT_CHILD_RUN => {
            let Ok(record) = serde_json::from_value::<TrajectoryChildRunRecord>(payload.clone())
            else {
                return;
            };
            let run = decoded
                .runs
                .entry(record.identity.root_input_id.clone())
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
                .entry(record.identity.root_input_id.clone())
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
                .entry(record.identity.root_input_id.clone())
                .or_default();
            run.finished_at = Some(record.finished_at);
            run.terminal = Some(record.kind);
            run.records.push(TrajectoryRecord::Terminal(record));
        }
        _ => {}
    }
}
