// Large-session paging and lazy detail for F-52 / D-69.

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

fn query_for(path: &std::path::Path, session_id: &str) -> SessionHistoryQuery {
    let paths = Arc::new(Mutex::new(HashMap::from([(
        session_id.to_string(),
        path.to_path_buf(),
    )])));
    SessionHistoryQuery::new(paths, Arc::new(FsSessionStoreFactory), None)
}

fn admit_root(
    store: &piko_session_store::SessionStore,
    expected: u64,
    agent: &str,
    input_id: &str,
    body: &str,
) {
    store
        .append(
            expected,
            piko_session_store::ProposedCommit::one(
                input_id,
                expected as i64 + 1,
                piko_session_store::RawEvent::new(
                    input_id,
                    EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                        input: AgentInput {
                            input_id: input_id.into(),
                            request_id: input_id.into(),
                            session_id: "s1".into(),
                            agent_instance_id: agent.into(),
                            origin: AgentInputOrigin::User,
                            delivery: AgentInputDelivery::StartWhenIdle,
                            content: MessageContent::String(body.into()),
                            submitted_at: expected as i64 + 1,
                            caller_agent_instance_id: None,
                            detached_recipient_agent_instance_id: None,
                        },
                        disposition: AgentInputDisposition::AppliedAsRoot,
                        root_input_id: Some(input_id.into()),
                        admitted_at: expected as i64 + 1,
                    }),
                )
                .unwrap(),
            ),
        )
        .unwrap();
}

#[tokio::test]
async fn overview_pages_a_large_session_without_copying_full_bodies() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    let body = "x".repeat(4_000);
    for index in 0..80 {
        admit_root(
            &opened.store,
            1 + index,
            agent,
            &format!("input-{index}"),
            &body,
        );
    }

    let query = query_for(temp.path(), "s1");
    let first = query.overview("s1", None, Some(20)).await.unwrap();
    assert_eq!(first.works.len(), 20);
    assert!(first.next_cursor.is_some());
    assert!(
        first
            .works
            .iter()
            .all(|work| work.input_preview.chars().count() <= 160)
    );
    assert!(
        first
            .works
            .iter()
            .all(|work| !work.input_preview.contains(&body))
    );

    let second = query
        .overview("s1", first.next_cursor.as_deref(), Some(20))
        .await
        .unwrap();
    assert_eq!(second.works.len(), 20);
    assert_ne!(second.works[0].root_input_id, first.works[0].root_input_id);

    let mut seen = first.works.len() + second.works.len();
    let mut cursor = second.next_cursor.clone();
    while let Some(next) = cursor {
        let page = query.overview("s1", Some(&next), Some(20)).await.unwrap();
        seen += page.works.len();
        cursor = page.next_cursor;
    }
    assert_eq!(seen, 80);

    let work = query
        .work_page("s1", "input-0", first.revision, None, Some(20))
        .await
        .unwrap();
    let input = work
        .items
        .iter()
        .find(|item| item.kind.0 == "input")
        .unwrap();
    assert!(!input.summary.contains(&body));
    let detail = query.item_detail("s1", &input.item_ref).await.unwrap();
    match detail.content {
        Some(HistoryItemContent::Input { input }) => match input.content {
            MessageContent::String(text) => assert_eq!(text, body),
            other => panic!("expected string body, got {other:?}"),
        },
        other => panic!("expected input detail, got {other:?}"),
    }

    let journal = query
        .journal_page(
            "s1",
            first.revision,
            None,
            Some(10),
            HistoryProvenanceFilter::Facts,
        )
        .await
        .unwrap();
    assert_eq!(journal.commits.len(), 10);
    assert!(journal.next_cursor.is_some());
    assert!(
        journal
            .commits
            .iter()
            .all(|commit| !commit.events.is_empty())
    );
}

fn append_event(
    store: &piko_session_store::SessionStore,
    expected: u64,
    commit_id: &str,
    event: piko_session_store::RawEvent,
) {
    store
        .append(
            expected,
            piko_session_store::ProposedCommit::one(commit_id, expected as i64 + 1, event),
        )
        .unwrap();
}

#[tokio::test]
async fn usage_correction_and_failed_work_survive_aligned_reads() {
    let temp = tempfile::tempdir().unwrap();
    SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
    let opened = piko_session_store::SessionStore::open(temp.path(), Default::default()).unwrap();
    let agent = "agent_s1_root";
    admit_root(&opened.store, 1, agent, "input-1", "failing work");
    append_event(
        &opened.store,
        2,
        "start",
        piko_session_store::RawEvent::new(
            "start",
            EventData::AgentInputProcessingStartedV1(
                piko_session_store::AgentInputProcessingStartedV1 {
                    agent_instance_id: agent.into(),
                    root_input_id: "input-1".into(),
                    request_id: "input-1".into(),
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
    append_event(
        &opened.store,
        3,
        "usage",
        piko_session_store::RawEvent::new(
            "usage",
            EventData::UsageRecorded(piko_session_store::UsageRecordedV1 {
                usage_id: "usage-1".into(),
                attribution: piko_session_store::UsageAttribution {
                    session_id: "s1".into(),
                    agent_instance_id: agent.into(),
                    root_input_id: "input-1".into(),
                    model_step_id: "step-1".into(),
                },
                provider: "scripted".into(),
                model_id: "scripted-model".into(),
                api_surface: None,
                pricing_policy_id: None,
                pricing_revision: None,
                usage: {
                    let mut usage = piko_protocol::Usage::empty();
                    usage.input = 10;
                    usage
                },
                incurred: true,
            }),
        )
        .unwrap(),
    );
    append_event(
        &opened.store,
        4,
        "correct",
        piko_session_store::RawEvent::new(
            "correct",
            EventData::UsageCorrected(piko_session_store::UsageCorrectedV1 {
                correction_id: "correction-1".into(),
                usage_id: "usage-1".into(),
                replacement: {
                    let mut usage = piko_protocol::Usage::empty();
                    usage.input = 12;
                    usage
                },
                reason: "provider correction".into(),
            }),
        )
        .unwrap(),
    );
    append_event(
        &opened.store,
        5,
        "finish",
        piko_session_store::RawEvent::new(
            "finish",
            EventData::AgentInputProcessingFinishedV1(
                piko_session_store::AgentInputProcessingFinishedV1 {
                    agent_instance_id: agent.into(),
                    root_input_id: "input-1".into(),
                    report: piko_protocol::AgentWorkReport {
                        agent_instance_id: agent.into(),
                        root_input_id: "input-1".into(),
                        report_id: "report-1".into(),
                        outcome: piko_protocol::AgentWorkOutcome::failed("model failed"),
                        summary: "failed".into(),
                        usage: {
                            let mut usage = piko_protocol::Usage::empty();
                            usage.input = 12;
                            usage
                        },
                        artifacts: Vec::new(),
                    },
                    finished_at: 6,
                },
            ),
        )
        .unwrap(),
    );

    let query = query_for(temp.path(), "s1");
    let overview = query.overview("s1", None, Some(10)).await.unwrap();
    assert_eq!(
        overview.works[0].outcome,
        Some(piko_protocol::AgentWorkProcessingStatus::Failed)
    );
    let page = query
        .work_page("s1", "input-1", overview.revision, None, Some(20))
        .await
        .unwrap();
    assert!(page.items.iter().any(|item| item.kind.0 == "usage"));
    assert!(page.items.iter().any(|item| item.kind.0 == "report"));
    std::fs::rename(
        temp.path().join("events"),
        temp.path().join("hidden-events"),
    )
    .unwrap();
    let again = query
        .work_page("s1", "input-1", overview.revision, None, Some(20))
        .await
        .unwrap();
    assert_eq!(again.items, page.items);
}
