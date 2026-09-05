// Test fixtures may construct concrete adapters for an on-disk journal.

use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::{
    AgentInput, AgentInputDelivery, AgentInputDisposition, AgentInputOrigin, HistoryItemContent,
    HistoryProvenanceFilter, MessageContent,
};
use piko_session_store::{AgentInputAdmittedV1, EventData};
use tokio::sync::Mutex;

use piko_hostd::adapters::storage::FsSessionStoreFactory;
use piko_hostd::application::SessionHistoryQuery;
use piko_hostd::infra::storage::session_store::SessionStore;

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
                piko_session_store::RawEvent::new(
                    "input-admitted",
                    EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                        input: AgentInput {
                            input_id: "input-1".into(),
                            request_id: "request-1".into(),
                            session_id: "history-session".into(),
                            agent_instance_id: agent_instance_id.into(),
                            origin: AgentInputOrigin::User,
                            delivery: AgentInputDelivery::StartWhenIdle,
                            content: MessageContent::String("explain the session".into()),
                            submitted_at: 2,
                            caller_agent_instance_id: None,
                            detached_recipient_agent_instance_id: None,
                        },
                        disposition: AgentInputDisposition::AppliedAsRoot,
                        root_input_id: Some("input-1".into()),
                        admitted_at: 2,
                    }),
                )
                .unwrap(),
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
    let query = SessionHistoryQuery::new(paths, Arc::new(FsSessionStoreFactory), None);

    let overview = query
        .overview("history-session", None, Some(10))
        .await
        .unwrap();
    assert_eq!(overview.cwd, "/project");
    assert_eq!(overview.works.len(), 1);
    assert_eq!(overview.works[0].root_input_id, "input-1");

    let journal = query
        .journal_page(
            "history-session",
            overview.revision,
            None,
            Some(10),
            HistoryProvenanceFilter::Facts,
        )
        .await
        .unwrap();
    let input_item = journal
        .commits
        .iter()
        .flat_map(|commit| &commit.events)
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
async fn work_and_journal_pages_reject_revision_drift() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let paths = Arc::new(Mutex::new(HashMap::from([(
        "s1".to_string(),
        temp.path().to_path_buf(),
    )])));
    let query = SessionHistoryQuery::new(paths, Arc::new(FsSessionStoreFactory), None);
    let error = query
        .journal_page("s1", 0, None, Some(10), HistoryProvenanceFilter::All)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("history revision changed"));
}

fn query_for(path: &std::path::Path, session_id: &str) -> SessionHistoryQuery {
    let paths = Arc::new(Mutex::new(HashMap::from([(
        session_id.to_string(),
        path.to_path_buf(),
    )])));
    SessionHistoryQuery::new(paths, Arc::new(FsSessionStoreFactory), None)
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
async fn work_page_attaches_matching_diagnostics_as_children() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    append(
        &opened.store,
        1,
        "input",
        2,
        piko_session_store::RawEvent::new(
            "input-admitted",
            EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                input: AgentInput {
                    input_id: "input-1".into(),
                    request_id: "request-1".into(),
                    session_id: "s1".into(),
                    agent_instance_id: agent.into(),
                    origin: AgentInputOrigin::User,
                    delivery: AgentInputDelivery::StartWhenIdle,
                    content: MessageContent::String("hello".into()),
                    submitted_at: 2,
                    caller_agent_instance_id: None,
                    detached_recipient_agent_instance_id: None,
                },
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: Some("input-1".into()),
                admitted_at: 2,
            }),
        )
        .unwrap(),
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
        "orphan-step",
        4,
        piko_session_store::RawEvent::optional(
            "orphan-step",
            "trajectory.model_step",
            serde_json::json!({
                "identity": {
                    "sessionId": "s1",
                    "agentInstanceId": agent,
                    "rootInputId": "input-1"
                },
                "stepId": "missing-step"
            }),
        ),
    );

    let query = query_for(temp.path(), "s1");
    let page = query
        .work_page("s1", "input-1", 4, None, Some(20))
        .await
        .unwrap();
    assert!(
        page.items
            .iter()
            .all(|item| item.provenance == piko_protocol::HistoryProvenance::Fact)
    );
    let input = page
        .items
        .iter()
        .find(|item| item.kind.0 == "input")
        .unwrap();
    assert_eq!(input.children.len(), 1);
    assert_eq!(input.children[0].kind.0, "prompt_assembly");
    assert!(!page.items.iter().any(|item| item.kind.0 == "diagnostic"));
}

#[tokio::test]
async fn transcript_order_is_independent_from_work_order() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    append(
        &opened.store,
        1,
        "input",
        2,
        piko_session_store::RawEvent::new(
            "input-admitted",
            EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                input: AgentInput {
                    input_id: "input-1".into(),
                    request_id: "request-1".into(),
                    session_id: "s1".into(),
                    agent_instance_id: agent.into(),
                    origin: AgentInputOrigin::User,
                    delivery: AgentInputDelivery::StartWhenIdle,
                    content: MessageContent::String("hello".into()),
                    submitted_at: 2,
                    caller_agent_instance_id: None,
                    detached_recipient_agent_instance_id: None,
                },
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: Some("input-1".into()),
                admitted_at: 2,
            }),
        )
        .unwrap(),
    );
    append(
        &opened.store,
        2,
        "message",
        3,
        piko_session_store::RawEvent::new(
            "message",
            EventData::MessageCommitted(piko_session_store::MessageCommittedV1 {
                message_id: "msg-1".into(),
                agent_instance_id: agent.into(),
                agent_parent_message_id: None,
                tree_parent_entry_id: None,
                root_input_id: Some("input-1".into()),
                committed_at: 3,
                message: piko_protocol::Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(3),
                },
            }),
        )
        .unwrap(),
    );
    append(
        &opened.store,
        3,
        "tree",
        4,
        piko_session_store::RawEvent::new(
            "tree",
            EventData::TreeEntryRecorded(piko_session_store::TreeEntryRecordedV1 {
                entry_id: "tree-1".into(),
                parent_entry_id: None,
                entry_type: "label".into(),
                timestamp: 4,
                payload: serde_json::json!({
                    "type": "label",
                    "id": "tree-1",
                    "parentId": null,
                    "timestamp": "4",
                    "text": "branch"
                }),
            }),
        )
        .unwrap(),
    );

    let query = query_for(temp.path(), "s1");
    let work = query
        .work_page("s1", "input-1", 4, None, Some(20))
        .await
        .unwrap();
    let transcript = query
        .transcript_page("s1", 4, None, Some(20))
        .await
        .unwrap();
    let work_kinds: Vec<_> = work.items.iter().map(|item| item.kind.0.as_str()).collect();
    let transcript_kinds: Vec<_> = transcript
        .items
        .iter()
        .map(|item| item.kind.0.as_str())
        .collect();
    assert_eq!(work_kinds.first().copied(), Some("input"));
    assert_eq!(transcript_kinds.first().copied(), Some("tree_entry"));
    assert!(transcript_kinds.contains(&"message"));
    assert!(!transcript_kinds.contains(&"input"));
}

#[tokio::test]
async fn child_without_origin_fact_is_unavailable() {
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
    let overview = query_for(temp.path(), "s1")
        .overview("s1", None, Some(10))
        .await
        .unwrap();
    let child = overview
        .agents
        .iter()
        .find(|agent| agent.agent_instance_id == "child")
        .unwrap();
    assert!(child.origin.is_none());
    assert!(matches!(
        child.origin_availability,
        piko_protocol::HistoryAvailability::Unavailable { .. }
    ));
}
