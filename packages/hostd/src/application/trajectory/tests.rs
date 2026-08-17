use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_MODEL_STEP, TrajectoryAssemblyRecord,
    TrajectoryIdentity, TrajectoryModelStepRecord, TrajectoryRecord, TrajectoryTerminalKind,
    TrajectoryTerminalRecord,
};
use piko_session_store::{EventData, ExecutionStartedV1};
use tokio::sync::Mutex;

use super::*;
use crate::application::trajectory::decode::run_usage;
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
    store
        .append_optional_event(
            "t-terminal",
            5,
            piko_protocol::TRAJECTORY_EVENT_TERMINAL,
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
        TrajectoryRecord::ModelStep(step) if step.step_id == "step-1"
    ));
    assert!(matches!(
        &run.records[1],
        TrajectoryRecord::Terminal(TrajectoryTerminalRecord {
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

    // Incremental: a later record is folded into the cached projection.
    let step2 = TrajectoryModelStepRecord {
        identity: assembly.identity,
        step_id: "step-2".into(),
        provider: "test".into(),
        model: "test-model".into(),
        request: serde_json::json!({"input": "again"}),
        options: serde_json::json!({}),
        started_at: 16,
        finished_at: Some(17),
        duration_ms: Some(1),
        retries: Vec::new(),
        fallback: None,
        response: None,
        message_id: None,
        usage: None,
    };
    store
        .append_optional_event(
            "t-step-2",
            6,
            TRAJECTORY_EVENT_MODEL_STEP,
            serde_json::to_value(&step2).unwrap(),
        )
        .unwrap();
    let run = query
        .fetch_run("s1", "run-1", &HashMap::new())
        .await
        .unwrap();
    assert_eq!(run.summary.step_count, 2);
    assert_eq!(run.records.len(), 3);
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
        TrajectoryRecord::SystemNotification(piko_protocol::TrajectorySystemNotificationRecord {
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
