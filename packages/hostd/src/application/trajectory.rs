//! Read-only trajectory query (F-36).
//!
//! Replays the session journal's raw events — acknowledged facts plus
//! optional `trajectory.*` records — and joins them by run identity into
//! run summaries and full run records for the web viewer. Queries never
//! mutate session state or invoke the model gateway.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_CHILD_RUN, TRAJECTORY_EVENT_MODEL_STEP,
    TRAJECTORY_EVENT_SYSTEM_NOTIFICATION, TRAJECTORY_EVENT_TERMINAL, TRAJECTORY_EVENT_TOOL_CALL,
    TrajectoryAssemblyRecord, TrajectoryChildRunRecord, TrajectoryCostTotal, TrajectoryMessage,
    TrajectoryModelStepRecord, TrajectoryRecord, TrajectoryRun, TrajectoryRunListPage,
    TrajectoryRunSummary, TrajectoryRunUsage, TrajectorySystemNotificationRecord,
    TrajectoryTerminalKind, TrajectoryTerminalRecord, TrajectoryToolCallRecord,
};
use piko_session_store::EventData;
use tokio::sync::Mutex;

use crate::api::ProtocolError;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;
use crate::ports::storage_types::RawJournalEventRef;

const DEFAULT_RUN_LIMIT: usize = 50;
const MAX_RUN_LIMIT: usize = 200;
const CURSOR_PREFIX: &str = "run:";

#[derive(Clone)]
pub struct TrajectoryQuery {
    pub(crate) session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub(crate) store_factory: Arc<dyn SessionStoreFactory>,
    pub(crate) storage: Option<Arc<dyn SessionRepositoryPort>>,
}

impl TrajectoryQuery {
    pub fn new(
        session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
        store_factory: Arc<dyn SessionStoreFactory>,
        storage: Option<Arc<dyn SessionRepositoryPort>>,
    ) -> Self {
        Self {
            session_paths,
            store_factory,
            storage,
        }
    }

    async fn session_dir(&self, session_id: &str) -> Result<PathBuf, ProtocolError> {
        if let Some(path) = self.session_paths.lock().await.get(session_id).cloned() {
            return Ok(path);
        }
        // Resume-friendly fallback: resolve persisted sessions through the
        // repository even when this hostd process has not opened them.
        if let Some(storage) = &self.storage {
            let all = storage
                .list(None)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            if let Some(persisted) = all
                .into_iter()
                .find(|session| session.state.session_id == session_id)
            {
                return Ok(persisted.path);
            }
        }
        Err(ProtocolError::InvalidCommand(format!(
            "trajectory unavailable for session {session_id}"
        )))
    }

    async fn load_events(
        &self,
        session_id: &str,
    ) -> Result<Vec<RawJournalEventRef>, ProtocolError> {
        let session_dir = self.session_dir(session_id).await?;
        let store = self.store_factory.open(&session_dir);
        store
            .raw_journal_events()
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))
    }

    /// List runs, newest first, bounded and cursor-paged.
    pub async fn list_runs(
        &self,
        session_id: &str,
        agent_instance_id: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
        dropped: &HashMap<String, u32>,
    ) -> Result<TrajectoryRunListPage, ProtocolError> {
        let events = self.load_events(session_id).await?;
        let decoded = decode_events(&events);
        let limit = if limit == 0 { DEFAULT_RUN_LIMIT } else { limit }.min(MAX_RUN_LIMIT);
        let mut runs = decoded
            .into_iter()
            .filter(|(_, run)| {
                agent_instance_id
                    .is_none_or(|agent| run.agent_instance_id.as_deref() == Some(agent))
            })
            .map(|(run_id, run)| {
                let summary = summarize(session_id, &run_id, &run, dropped);
                (run_id, summary)
            })
            .collect::<Vec<_>>();
        runs.sort_by(|(left_id, left), (right_id, right)| {
            right
                .started_at
                .cmp(&left.started_at)
                .then_with(|| right_id.cmp(left_id))
        });
        let after = cursor.and_then(|value| value.strip_prefix(CURSOR_PREFIX));
        let mut iter = runs.into_iter().peekable();
        if let Some(after_run) = after {
            while iter
                .peek()
                .is_some_and(|(run_id, _)| run_id.as_str() != after_run)
            {
                iter.next();
            }
            iter.next(); // resume after the cursor run
        }
        let mut page = Vec::new();
        for (_, summary) in iter.by_ref() {
            if page.len() >= limit {
                break;
            }
            page.push(summary);
        }
        let next_cursor = iter
            .peek()
            .map(|(run_id, _)| format!("{CURSOR_PREFIX}{run_id}"));
        Ok(TrajectoryRunListPage {
            runs: page,
            next_cursor,
        })
    }

    /// Fetch one full run record.
    pub async fn fetch_run(
        &self,
        session_id: &str,
        run_id: &str,
        dropped: &HashMap<String, u32>,
    ) -> Result<TrajectoryRun, ProtocolError> {
        let events = self.load_events(session_id).await?;
        let decoded = decode_events(&events);
        let (run_id, run) = decoded
            .into_iter()
            .find(|(candidate, _)| candidate == run_id)
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!("trajectory run {run_id} not found"))
            })?;
        Ok(TrajectoryRun {
            summary: summarize(session_id, &run_id, &run, dropped),
            assembly: run.assembly,
            records: run.records,
            messages: run.messages,
        })
    }
}

#[derive(Default)]
struct DecodedRun {
    agent_instance_id: Option<String>,
    execution_id: Option<String>,
    source_turn_id: Option<String>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    terminal: Option<TrajectoryTerminalKind>,
    assembly: Option<TrajectoryAssemblyRecord>,
    records: Vec<TrajectoryRecord>,
    messages: Vec<TrajectoryMessage>,
    step_count: u32,
    tool_call_count: u32,
    child_run_count: u32,
}

fn decode_events(events: &[RawJournalEventRef]) -> BTreeMap<String, DecodedRun> {
    let mut runs: BTreeMap<String, DecodedRun> = BTreeMap::new();
    let mut execution_to_run: HashMap<String, String> = HashMap::new();
    for raw in events {
        match raw.event_type.as_str() {
            "execution_started" => {
                let Ok(EventData::ExecutionStarted(started)) =
                    serde_json::from_value::<EventData>(raw.payload.clone())
                else {
                    continue;
                };
                execution_to_run.insert(started.execution_id.clone(), started.run_id.clone());
                let run = runs.entry(started.run_id.clone()).or_default();
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
                    continue;
                };
                let Some(run_id) = execution_to_run.get(&execution_id).cloned() else {
                    continue;
                };
                let run = runs.entry(run_id).or_default();
                run.finished_at = Some(finished_at);
                run.terminal = Some(match report.outcome {
                    piko_protocol::ExecutionOutcome::Succeeded { .. } => {
                        TrajectoryTerminalKind::Completed
                    }
                    piko_protocol::ExecutionOutcome::Failed { .. } => {
                        TrajectoryTerminalKind::Failed
                    }
                    piko_protocol::ExecutionOutcome::Cancelled { .. } => {
                        TrajectoryTerminalKind::Cancelled
                    }
                });
            }
            "message_committed" => {
                let Ok(EventData::MessageCommitted(committed)) =
                    serde_json::from_value::<EventData>(raw.payload.clone())
                else {
                    continue;
                };
                let Some(execution_id) = committed.execution_id else {
                    continue;
                };
                let Some(run_id) = execution_to_run.get(&execution_id).cloned() else {
                    continue;
                };
                runs.entry(run_id)
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
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                run.agent_instance_id
                    .get_or_insert_with(|| record.identity.agent_instance_id.to_string());
                run.assembly = Some(record);
            }
            TRAJECTORY_EVENT_MODEL_STEP => {
                let Ok(record) =
                    serde_json::from_value::<TrajectoryModelStepRecord>(raw.payload.clone())
                else {
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                run.step_count += 1;
                run.records.push(TrajectoryRecord::ModelStep(record));
            }
            TRAJECTORY_EVENT_TOOL_CALL => {
                let Ok(record) =
                    serde_json::from_value::<TrajectoryToolCallRecord>(raw.payload.clone())
                else {
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                run.tool_call_count += 1;
                run.records.push(TrajectoryRecord::ToolCall(record));
            }
            TRAJECTORY_EVENT_CHILD_RUN => {
                let Ok(record) =
                    serde_json::from_value::<TrajectoryChildRunRecord>(raw.payload.clone())
                else {
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                run.child_run_count += 1;
                run.records.push(TrajectoryRecord::ChildRun(record));
            }
            TRAJECTORY_EVENT_SYSTEM_NOTIFICATION => {
                let Ok(record) = serde_json::from_value::<TrajectorySystemNotificationRecord>(
                    raw.payload.clone(),
                ) else {
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                run.records
                    .push(TrajectoryRecord::SystemNotification(record));
            }
            TRAJECTORY_EVENT_TERMINAL => {
                let Ok(record) =
                    serde_json::from_value::<TrajectoryTerminalRecord>(raw.payload.clone())
                else {
                    continue;
                };
                let run = runs.entry(record.identity.run_id.clone()).or_default();
                // Mirrors the `execution_finished` fact (same source outcome),
                // so a run whose terminal record is present decodes to the
                // same summary even if the fact falls outside the page window.
                run.finished_at = Some(record.finished_at);
                run.terminal = Some(record.kind);
                run.records.push(TrajectoryRecord::Terminal(record));
            }
            _ => {}
        }
    }
    runs
}

fn summarize(
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
fn run_usage(records: &[TrajectoryRecord]) -> Option<TrajectoryRunUsage> {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use piko_protocol::{
        TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_MODEL_STEP, TrajectoryAssemblyRecord,
        TrajectoryIdentity, TrajectoryModelStepRecord, TrajectoryTerminalKind,
    };
    use piko_session_store::{EventData, ExecutionStartedV1};
    use tokio::sync::Mutex;

    use super::*;
    use crate::infra::storage::JsonlSessionRepository;
    use crate::infra::storage::session_store::SessionStore;

    #[tokio::test]
    async fn query_lists_and_fetches_runs_from_journal_events() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
        let execution_id = "exec-1".to_string();
        let agent_instance_id = "agent_s1_root";
        store
            .commit_events(
                "ex-start",
                1,
                vec![EventData::ExecutionStarted(ExecutionStartedV1 {
                    run_id: "run-1".into(),
                    execution_id: execution_id.clone(),
                    request_id: "req-1".into(),
                    agent_instance_id: agent_instance_id.into(),
                    admitted_revision: 0,
                    base_message_id: None,
                    tree_base_entry_id: None,
                    source_turn_id: Some("turn-1".into()),
                    detached_recipient_agent_instance_id: None,
                    prompt_assembly_version: 5,
                    prompt_digest: "digest".into(),
                    started_at: 10,
                })],
            )
            .unwrap();
        let assembly = TrajectoryAssemblyRecord {
            identity: TrajectoryIdentity {
                session_id: "s1".into(),
                agent_instance_id: agent_instance_id.into(),
                run_id: "run-1".into(),
                execution_id: None,
                source_turn_id: None,
            },
            assembly_version: 5,
            prompt_digest: "digest".into(),
            prompt: piko_protocol::SemanticRunPrompt::default(),
            tool_catalog: piko_protocol::ResolvedToolCatalog::new(Vec::new(), "tools"),
            recorded_at: 11,
        };
        store
            .append_optional_event(
                "t-assembly",
                2,
                TRAJECTORY_EVENT_ASSEMBLY,
                serde_json::to_value(&assembly).unwrap(),
            )
            .unwrap();
        let step = TrajectoryModelStepRecord {
            identity: assembly.identity.clone(),
            step_id: "step-1".into(),
            provider: "test".into(),
            model: "test-model".into(),
            request: serde_json::json!({"input": "hi"}),
            options: serde_json::json!({}),
            started_at: 12,
            finished_at: Some(13),
            duration_ms: Some(1),
            retries: Vec::new(),
            fallback: None,
            response: None,
            message_id: None,
            usage: None,
        };
        store
            .append_optional_event(
                "t-step",
                3,
                TRAJECTORY_EVENT_MODEL_STEP,
                serde_json::to_value(&step).unwrap(),
            )
            .unwrap();
        store
            .commit_events(
                "ex-finish",
                4,
                vec![EventData::ExecutionFinished {
                    execution_id,
                    report: piko_protocol::AgentRunReport {
                        agent_instance_id: agent_instance_id.into(),
                        report_id: "report-1".into(),
                        outcome: piko_protocol::ExecutionOutcome::Succeeded {
                            usage: piko_protocol::Usage::empty(),
                        },
                        summary: "done".into(),
                        usage: piko_protocol::Usage::empty(),
                        artifacts: Vec::new(),
                    },
                    finished_at: 14,
                }],
            )
            .unwrap();
        // The terminal record mirrors the fact: it is decoded into the run
        // records and keeps the summary terminal consistent.
        store
            .append_optional_event(
                "t-terminal",
                5,
                TRAJECTORY_EVENT_TERMINAL,
                serde_json::to_value(TrajectoryTerminalRecord {
                    identity: assembly.identity.clone(),
                    kind: TrajectoryTerminalKind::Completed,
                    reason: None,
                    finished_at: 15,
                })
                .unwrap(),
            )
            .unwrap();

        let query = TrajectoryQuery::new(
            Arc::new(Mutex::new(HashMap::from([(
                "s1".to_string(),
                temp.path().to_path_buf(),
            )]))),
            Arc::new(crate::adapters::storage::FsSessionStoreFactory),
            None,
        );
        let page = query
            .list_runs("s1", None, None, 50, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(page.runs.len(), 1);
        assert_eq!(page.runs[0].run_id, "run-1");
        assert_eq!(
            page.runs[0].terminal,
            Some(TrajectoryTerminalKind::Completed)
        );
        assert_eq!(page.runs[0].step_count, 1);
        assert_eq!(page.runs[0].source_turn_id.as_deref(), Some("turn-1"));

        let run = query
            .fetch_run("s1", "run-1", &HashMap::new())
            .await
            .unwrap();
        assert!(run.assembly.is_some());
        assert_eq!(run.records.len(), 2);
        assert!(matches!(
            &run.records[0],
            piko_protocol::TrajectoryRecord::ModelStep(step) if step.step_id == "step-1"
        ));
        assert!(matches!(
            &run.records[1],
            piko_protocol::TrajectoryRecord::Terminal(piko_protocol::TrajectoryTerminalRecord {
                kind: TrajectoryTerminalKind::Completed,
                ..
            })
        ));
        assert_eq!(run.summary.finished_at, Some(15));
        assert!(
            query
                .fetch_run("s1", "missing", &HashMap::new())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn query_resolves_persisted_session_without_session_paths_entry() {
        let temp = tempfile::tempdir().unwrap();
        let repo = JsonlSessionRepository::new(temp.path().to_path_buf());
        let persisted = repo.create("/project").unwrap();
        let session_id = persisted.state.session_id.clone();
        let store = SessionStore::new(&persisted.path);
        let assembly = TrajectoryAssemblyRecord {
            identity: TrajectoryIdentity {
                session_id: session_id.clone(),
                agent_instance_id: format!("agent_{session_id}_root"),
                run_id: "run-1".into(),
                execution_id: None,
                source_turn_id: None,
            },
            assembly_version: 5,
            prompt_digest: "digest".into(),
            prompt: piko_protocol::SemanticRunPrompt::default(),
            tool_catalog: piko_protocol::ResolvedToolCatalog::new(Vec::new(), "tools"),
            recorded_at: 1,
        };
        store
            .append_optional_event(
                "t-assembly",
                2,
                TRAJECTORY_EVENT_ASSEMBLY,
                serde_json::to_value(&assembly).unwrap(),
            )
            .unwrap();

        // session_paths is empty: the query must fall back to the repository.
        let query = TrajectoryQuery::new(
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(crate::adapters::storage::FsSessionStoreFactory),
            Some(Arc::new(repo)),
        );
        let page = query
            .list_runs(&session_id, None, None, 50, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(
            page.runs.len(),
            1,
            "persisted session resolved via repository"
        );
        assert_eq!(page.runs[0].run_id, "run-1");
    }

    #[test]
    fn run_usage_rolls_up_model_steps_host_side() {
        let identity = TrajectoryIdentity {
            session_id: "s".into(),
            agent_instance_id: "a".into(),
            run_id: "r".into(),
            execution_id: None,
            source_turn_id: None,
        };
        let usage = piko_protocol::Usage {
            input: 1000,
            output: 100,
            cache_read: 800,
            cache_write: 200,
            total_tokens: 1100,
            units: Default::default(),
            cost: piko_protocol::UsageCost {
                entries: vec![piko_protocol::UsageCostEntry {
                    currency: "USD".into(),
                    basis: piko_protocol::UsageCostBasis::ListPrice,
                    components: Default::default(),
                    total: 0.01,
                }],
            },
        };
        let step = TrajectoryModelStepRecord {
            identity,
            step_id: "step-1".into(),
            provider: "test".into(),
            model: "m".into(),
            request: serde_json::json!({}),
            options: serde_json::json!({}),
            started_at: 1,
            finished_at: Some(2),
            duration_ms: Some(1),
            retries: Vec::new(),
            fallback: None,
            response: None,
            message_id: Some("msg-1".into()),
            usage: Some(Box::new(usage)),
        };
        let records = vec![
            TrajectoryRecord::ModelStep(step),
            TrajectoryRecord::SystemNotification(TrajectorySystemNotificationRecord {
                identity: TrajectoryIdentity {
                    session_id: "s".into(),
                    agent_instance_id: "a".into(),
                    run_id: "r".into(),
                    execution_id: None,
                    source_turn_id: None,
                },
                kind: piko_protocol::TrajectoryNotificationKind::RunError,
                summary: "boom".into(),
                recorded_at: 1,
            }),
        ];

        let rolled = run_usage(&records).expect("rollup present");
        assert_eq!(rolled.input, 1000);
        assert_eq!(rolled.output, 100);
        assert_eq!(rolled.cache_read, 800);
        assert_eq!(rolled.cache_write, 200);
        assert_eq!(rolled.cache_hit_ratio, Some(0.8));
        assert_eq!(rolled.cost.len(), 1);
        assert_eq!(rolled.cost[0].currency, "USD");
        assert!((rolled.cost[0].total - 0.01).abs() < f64::EPSILON);

        assert!(run_usage(&[]).is_none());
        assert!(run_usage(&records[1..]).is_none());
    }
}
