//! C4 / C8 crash-point inventory for AgentInput facts.

use std::collections::BTreeMap;

use piko_protocol::{AgentInstanceIdentity, MessageContent};
use piko_session_store::{
    AgentInputAdmittedV1, AgentInputProcessingStartedV1, AgentPendingActionResolvedV1, EventData,
    NewSession, OpenOptions, ProposedCommit, RawEvent, SessionStore, StoreError,
};
use tempfile::tempdir;

fn new_session(id: &str) -> NewSession {
    NewSession {
        session_id: id.into(),
        cwd: "/project".into(),
        created_at: 1,
        root: AgentInstanceIdentity {
            session_id: id.into(),
            agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        },
    }
}

fn event(id: &str, data: EventData) -> RawEvent {
    RawEvent::new(id, data).unwrap()
}

fn user_input(id: &str, delivery: piko_protocol::AgentInputDelivery) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: id.into(),
        request_id: id.into(),
        session_id: "s1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery,
        content: MessageContent::String(id.into()),
        submitted_at: 2,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}

fn start_root(id: &str) -> ProposedCommit {
    ProposedCommit {
        commit_id: format!("start-{id}"),
        committed_at: 2,
        causation_id: None,
        correlation_id: None,
        events: vec![
            event(
                &format!("admit-{id}"),
                EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                    input: user_input(id, piko_protocol::AgentInputDelivery::StartWhenIdle),
                    disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
                    root_input_id: Some(id.into()),
                    admitted_at: 2,
                }),
            ),
            event(
                &format!("proc-{id}"),
                EventData::AgentInputProcessingStartedV1(AgentInputProcessingStartedV1 {
                    agent_instance_id: "root".into(),
                    root_input_id: id.into(),
                    request_id: id.into(),
                    base_message_id: None,
                    tree_base_entry_id: None,
                    detached_recipient_agent_instance_id: None,
                    prompt_assembly_version: 1,
                    prompt_digest: "digest".into(),
                    started_at: 2,
                }),
            ),
        ],
        extensions: BTreeMap::new(),
    }
}

#[test]
fn pending_steer_replay_freezes_captured_root_input_id() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened.store.append(1, start_root("root-1")).unwrap();
    opened
        .store
        .append(
            2,
            ProposedCommit::one(
                "admit-steer",
                3,
                event(
                    "admit-steer",
                    EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                        input: user_input(
                            "steer-1",
                            piko_protocol::AgentInputDelivery::SteerActive,
                        ),
                        disposition: piko_protocol::AgentInputDisposition::PendingSteer,
                        root_input_id: Some("root-1".into()),
                        admitted_at: 3,
                    }),
                ),
            ),
        )
        .unwrap();
    drop(opened);

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    let stored = reopened
        .aggregate
        .agent_inputs
        .get("steer-1")
        .expect("steer survived replay");
    assert_eq!(stored.root_input_id.as_deref(), Some("root-1"));
    assert_eq!(
        stored.disposition,
        piko_protocol::AgentInputDisposition::PendingSteer
    );
    assert_eq!(
        reopened
            .aggregate
            .active_root_by_agent
            .get("root")
            .map(String::as_str),
        Some("root-1")
    );
}

#[test]
fn unknown_pending_action_resolve_is_invalid_event() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    opened.store.append(1, start_root("root-1")).unwrap();
    let error = opened
        .store
        .append(
            2,
            ProposedCommit::one(
                "resolve-unknown",
                3,
                event(
                    "resolve-unknown",
                    EventData::AgentPendingActionResolvedV1(AgentPendingActionResolvedV1 {
                        agent_instance_id: "root".into(),
                        root_input_id: "root-1".into(),
                        action_id: "missing".into(),
                        resolved_at: 3,
                    }),
                ),
            ),
        )
        .expect_err("unknown action_id must not append");
    assert!(
        matches!(error, StoreError::InvalidEvent(ref message) if message.contains("unknown pending action")),
        "{error:?}"
    );
}
