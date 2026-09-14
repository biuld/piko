// Test fixtures may construct concrete adapters for an on-disk journal.

use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::{
    AgentInput, AgentInputDelivery, AgentInputDisposition, AgentInputOrigin, HistoryItemContent,
    MessageContent,
};
use piko_session_store::{AgentInputAdmittedV1, EventData};
use tokio::sync::Mutex;

use piko_hostd::adapters::storage::FsSessionStoreFactory;
use piko_hostd::application::SessionHistoryQuery;
use piko_hostd::infra::storage::session_store::SessionStore;

fn user_input(input_id: &str, session_id: &str, agent: &str, at: i64) -> AgentInput {
    AgentInput {
        input_id: input_id.into(),
        request_id: format!("request-{input_id}"),
        session_id: session_id.into(),
        agent_instance_id: agent.into(),
        origin: AgentInputOrigin::User,
        delivery: AgentInputDelivery::StartWhenIdle,
        content: MessageContent::String("explain the session".into()),
        submitted_at: at,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}

fn admitted_event(
    input_id: &str,
    session_id: &str,
    agent: &str,
    at: i64,
) -> piko_session_store::RawEvent {
    piko_session_store::RawEvent::new(
        "input-admitted",
        EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
            input: user_input(input_id, session_id, agent, at),
            disposition: AgentInputDisposition::AppliedAsRoot,
            root_input_id: Some(input_id.into()),
            admitted_at: at,
        }),
    )
    .unwrap()
}

#[tokio::test]
async fn unopened_session_is_inspected_without_attaching_it() {
    let temp = tempfile::tempdir().unwrap();
    let _host_store =
        SessionStore::create_session(temp.path(), "history-session".into(), "/project".into(), 1)
            .unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let store = opened.store;
    let agent_instance_id = "agent_history-session_root";
    store
        .append(
            1,
            piko_session_store::ProposedCommit::one(
                "input-commit",
                2,
                admitted_event("input-1", "history-session", agent_instance_id, 2),
            ),
        )
        .unwrap();

    let paths = Arc::new(Mutex::new(HashMap::from([(
        "history-session".to_string(),
        temp.path().to_path_buf(),
    )])));
    // The input is deliberately older than the published snapshot.
    store
        .append(
            2,
            piko_session_store::ProposedCommit::one(
                "later",
                3,
                piko_session_store::RawEvent::new(
                    "branch-selected",
                    EventData::BranchSelected {
                        selected_tree_entry_id: None,
                        root_base_message_id: None,
                    },
                )
                .unwrap(),
            ),
        )
        .unwrap();
    let query = SessionHistoryQuery::new(
        paths,
        Arc::new(FsSessionStoreFactory),
        None,
        Default::default(),
    );

    let overview = query.overview("history-session").await.unwrap();
    assert_eq!(overview.cwd, "/project");
    assert_eq!(overview.agents.len(), 1);
    assert_eq!(overview.agents[0].agent_instance_id, agent_instance_id);

    let stream = query
        .agent_stream(
            "history-session",
            agent_instance_id,
            overview.revision,
            None,
            Some(10),
        )
        .await
        .unwrap();
    let input_item = stream
        .items
        .iter()
        .find(|item| item.kind.0 == "input")
        .unwrap();
    let detail = query
        .item_detail("history-session", &input_item.item_ref)
        .await
        .unwrap();
    assert!(input_item.revision < overview.revision);
    assert_eq!(input_item.item_ref.revision, overview.revision);
    assert!(matches!(
        detail.content,
        Some(HistoryItemContent::Input { .. })
    ));
}

#[tokio::test]
async fn agent_stream_rejects_revision_drift() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let paths = Arc::new(Mutex::new(HashMap::from([(
        "s1".to_string(),
        temp.path().to_path_buf(),
    )])));
    let query = SessionHistoryQuery::new(
        paths,
        Arc::new(FsSessionStoreFactory),
        None,
        Default::default(),
    );
    let error = query
        .agent_stream("s1", "agent_s1_root", 0, None, Some(10))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("history revision changed"));
}

fn query_for(path: &std::path::Path, session_id: &str) -> SessionHistoryQuery {
    let paths = Arc::new(Mutex::new(HashMap::from([(
        session_id.to_string(),
        path.to_path_buf(),
    )])));
    SessionHistoryQuery::new(
        paths,
        Arc::new(FsSessionStoreFactory),
        None,
        Default::default(),
    )
}

fn append(
    store: &piko_session_store::SessionStore,
    expected: u64,
    commit_id: &str,
    at: i64,
    event: piko_session_store::RawEvent,
) {
    store
        .append(
            expected,
            piko_session_store::ProposedCommit::one(commit_id, at, event),
        )
        .unwrap();
}

#[tokio::test]
async fn stream_is_flat_journal_order_and_excludes_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    append(
        &opened.store,
        1,
        "input",
        2,
        admitted_event("input-1", "s1", agent, 2),
    );
    append(
        &opened.store,
        2,
        "assembly",
        3,
        piko_session_store::RawEvent::optional(
            "assembly",
            "trajectory.assembly",
            serde_json::json!({
                "identity": {
                    "sessionId": "s1",
                    "agentInstanceId": agent,
                    "rootInputId": "input-1"
                }
            }),
        ),
    );
    append(
        &opened.store,
        3,
        "message",
        4,
        piko_session_store::RawEvent::new(
            "message",
            EventData::MessageCommitted(piko_session_store::MessageCommittedV1 {
                message_id: "msg-1".into(),
                agent_instance_id: agent.into(),
                agent_parent_message_id: None,
                tree_parent_entry_id: None,
                root_input_id: Some("input-1".into()),
                committed_at: 4,
                message: piko_protocol::Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(4),
                },
            }),
        )
        .unwrap(),
    );
    append(
        &opened.store,
        4,
        "tree",
        5,
        piko_session_store::RawEvent::new(
            "tree",
            EventData::TreeEntryRecorded(piko_session_store::TreeEntryRecordedV1 {
                entry_id: "tree-1".into(),
                parent_entry_id: None,
                entry_type: "label".into(),
                timestamp: 5,
                payload: serde_json::json!({
                    "type": "label",
                    "id": "tree-1",
                    "parentId": null,
                    "timestamp": "5",
                    "text": "branch"
                }),
            }),
        )
        .unwrap(),
    );

    let query = query_for(temp.path(), "s1");
    let stream = query
        .agent_stream("s1", agent, 5, None, Some(20))
        .await
        .unwrap();
    let kinds: Vec<_> = stream
        .items
        .iter()
        .map(|item| item.kind.0.as_str())
        .collect();
    // Journal order across works; diagnostics and tree entries never appear.
    assert_eq!(kinds, vec!["input", "message"]);
    let input = stream.items.first().unwrap();
    assert_eq!(input.relation.root_input_id.as_deref(), Some("input-1"));
    assert_eq!(input.relation.input_id.as_deref(), Some("input-1"));
}

#[tokio::test]
async fn lane_summary_lists_steps_and_tool_calls() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    append(
        &opened.store,
        1,
        "input",
        2,
        admitted_event("input-1", "s1", agent, 2),
    );
    append(
        &opened.store,
        2,
        "start",
        3,
        piko_session_store::RawEvent::new(
            "start",
            EventData::AgentInputProcessingStartedV1(
                piko_session_store::AgentInputProcessingStartedV1 {
                    agent_instance_id: agent.into(),
                    root_input_id: "input-1".into(),
                    request_id: "request-input-1".into(),
                    base_message_id: None,
                    tree_base_entry_id: None,
                    detached_recipient_agent_instance_id: None,
                    prompt_assembly_version: 1,
                    prompt_digest: "digest".into(),
                    started_at: 3,
                },
            ),
        )
        .unwrap(),
    );
    // Assistant message, tool-call message, and the ModelStep are one atomic
    // journal commit, chained on the execution ancestry.
    let assistant = piko_session_store::RawEvent::new(
        "assistant",
        EventData::MessageCommitted(piko_session_store::MessageCommittedV1 {
            message_id: "msg-assist".into(),
            agent_instance_id: agent.into(),
            agent_parent_message_id: None,
            tree_parent_entry_id: None,
            root_input_id: Some("input-1".into()),
            committed_at: 4,
            message: piko_protocol::Message::Assistant {
                content: vec![],
                checkpoint: None,
                provider: "scripted".into(),
                model: "scripted-model".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: Some(4),
            },
        }),
    )
    .unwrap();
    let tool_call = piko_session_store::RawEvent::new(
        "tool-message",
        EventData::MessageCommitted(piko_session_store::MessageCommittedV1 {
            message_id: "tool-msg-1".into(),
            agent_instance_id: agent.into(),
            agent_parent_message_id: Some("msg-assist".into()),
            tree_parent_entry_id: Some("msg-assist".into()),
            root_input_id: Some("input-1".into()),
            committed_at: 4,
            message: piko_protocol::Message::ToolCall {
                id: "call-1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": "ls"}),
                model: None,
                provider: None,
                timestamp: Some(4),
            },
        }),
    )
    .unwrap();
    let step = piko_session_store::RawEvent::new(
        "step",
        EventData::ModelStepCommitted(piko_session_store::ModelStepCommittedV1 {
            model_step_id: "step-s1-1".into(),
            step_index: 1,
            root_input_id: "input-1".into(),
            agent_instance_id: agent.into(),
            assistant_message_id: "msg-assist".into(),
            tool_call_message_ids: vec!["tool-msg-1".into()],
            outcome: piko_protocol::ModelStepOutcome::ToolCalls,
            started_at: 3,
            finished_at: 4,
        }),
    )
    .unwrap();
    opened
        .store
        .append(
            3,
            piko_session_store::ProposedCommit {
                commit_id: "step".into(),
                committed_at: 5,
                causation_id: None,
                correlation_id: None,
                events: vec![assistant, tool_call, step],
                extensions: Default::default(),
            },
        )
        .unwrap();

    let query = query_for(temp.path(), "s1");
    let lane = query.lane_summary("s1", agent, 4).await.unwrap();
    assert!(!lane.timing_available);
    let kinds: Vec<_> = lane.blocks.iter().map(|block| block.kind).collect();
    assert_eq!(
        kinds,
        vec![
            piko_protocol::HistoryLaneBlockKind::ToolCall,
            piko_protocol::HistoryLaneBlockKind::ModelStep,
        ]
    );
    assert_eq!(lane.blocks[0].label, "bash");
    assert_eq!(lane.blocks[1].label, "step 1");

    let stream = query
        .agent_stream("s1", agent, 4, None, Some(20))
        .await
        .unwrap();
    let badges = stream
        .items
        .iter()
        .map(|item| item.badge.as_str())
        .collect::<Vec<_>>();
    assert_eq!(badges, vec!["USER", "ASSISTANT", "TOOL", "STEP 1"]);
}

#[tokio::test]
async fn overview_lists_child_agent_without_origin() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    append(
        &opened.store,
        1,
        "child",
        2,
        piko_session_store::RawEvent::new(
            "child",
            EventData::AgentCreated {
                identity: piko_protocol::AgentInstanceIdentity {
                    session_id: "s1".into(),
                    agent_instance_id: "child".into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some("agent_s1_root".into()),
                },
                spec: piko_protocol::AgentSpec {
                    id: "worker".into(),
                    version: "1".into(),
                    provenance: piko_protocol::PromptSource::new("test", "worker"),
                    name: "worker".into(),
                    role: "worker".into(),
                    kind: piko_protocol::AgentKind::Worker,
                    description: None,
                    base_instructions: String::new(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
                created_at: 2,
            },
        )
        .unwrap(),
    );
    let overview = query_for(temp.path(), "s1").overview("s1").await.unwrap();
    let child = overview
        .agents
        .iter()
        .find(|agent| agent.agent_instance_id == "child")
        .unwrap();
    assert_eq!(
        child.parent_agent_instance_id.as_deref(),
        Some("agent_s1_root")
    );
}
