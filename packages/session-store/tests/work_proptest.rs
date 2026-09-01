//! Projector invariants over valid AgentInput commit sequences (D-68 PR-4).

use std::collections::BTreeMap;

use piko_protocol::{
    AgentForeground, AgentInputDisposition, AgentInstanceIdentity, AgentWorkOutcome,
    AgentWorkReport, MessageContent, PendingActionSummary, Usage,
};
use piko_session_store::{
    AgentInputAdmittedV1, AgentInputDispositionChangedV1, AgentInputProcessingFinishedV1,
    AgentInputProcessingStartedV1, AgentInterruptRequestedV1, AgentPendingActionRequestedV1,
    AgentPendingActionResolvedV1, EventData, NewSession, ProposedCommit, RawEvent, SessionStore,
};
use proptest::prelude::*;
use tempfile::tempdir;

fn new_session() -> NewSession {
    NewSession {
        session_id: "s1".into(),
        cwd: "/project".into(),
        created_at: 1,
        root: AgentInstanceIdentity {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        },
    }
}

fn event(id: String, data: EventData) -> RawEvent {
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

fn append(store: &SessionStore, rev: u64, id: &str, data: EventData) {
    store
        .append(
            rev,
            ProposedCommit::one(id, rev as i64 + 2, event(id.into(), data)),
        )
        .unwrap();
}

fn append_start(store: &SessionStore, rev: u64, id: &str) {
    store
        .append(
            rev,
            ProposedCommit {
                commit_id: format!("start-{id}"),
                committed_at: 2,
                causation_id: None,
                correlation_id: None,
                events: vec![
                    event(
                        format!("admit-{id}"),
                        EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                            input: user_input(id, piko_protocol::AgentInputDelivery::FollowUp),
                            disposition: AgentInputDisposition::AppliedAsRoot,
                            root_input_id: Some(id.into()),
                            admitted_at: 2,
                        }),
                    ),
                    event(
                        format!("proc-{id}"),
                        EventData::AgentInputProcessingStartedV1(AgentInputProcessingStartedV1 {
                            agent_instance_id: "root".into(),
                            root_input_id: id.into(),
                            request_id: id.into(),
                            base_message_id: None,
                            tree_base_entry_id: None,
                            detached_recipient_agent_instance_id: None,
                            prompt_assembly_version: 1,
                            prompt_digest: "d".into(),
                            started_at: 2,
                        }),
                    ),
                ],
                extensions: BTreeMap::new(),
            },
        )
        .unwrap();
}

fn report(root: &str) -> AgentWorkReport {
    AgentWorkReport {
        agent_instance_id: "root".into(),
        root_input_id: root.into(),
        report_id: format!("report-{root}"),
        outcome: AgentWorkOutcome::Succeeded {
            usage: Usage::empty(),
        },
        summary: "ok".into(),
        usage: Usage::empty(),
        artifacts: Vec::new(),
    }
}

fn assert_invariants(store: &SessionStore) {
    let mut aggregate = store.aggregate();
    let rebuilt = {
        let mut clone = aggregate.clone();
        clone.rebuild_work_projection();
        clone
    };
    assert_eq!(aggregate.agent_work, rebuilt.agent_work);
    assert_eq!(aggregate.active_root_by_agent, rebuilt.active_root_by_agent);
    aggregate.rebuild_work_projection();

    let snapshot = aggregate.agent_work_snapshot("root");
    assert!(aggregate.active_root_by_agent.len() <= 1);
    assert_eq!(
        snapshot
            .active_work
            .as_ref()
            .map(|work| work.root_input_id.clone()),
        aggregate.active_root_by_agent.get("root").cloned()
    );

    let mut queued: Vec<_> = aggregate
        .agent_inputs
        .values()
        .filter(|input| {
            input.input.agent_instance_id == "root"
                && input.disposition == AgentInputDisposition::PendingFollowUp
        })
        .collect();
    queued.sort_by_key(|input| (input.admission_revision, input.input.input_id.clone()));
    assert_eq!(
        snapshot
            .queued_inputs
            .iter()
            .map(|input| input.input_id.as_str())
            .collect::<Vec<_>>(),
        queued
            .iter()
            .map(|input| input.input.input_id.as_str())
            .collect::<Vec<_>>()
    );

    let active = aggregate.active_root_by_agent.get("root");
    for input in aggregate.agent_inputs.values() {
        if input.disposition != AgentInputDisposition::PendingSteer
            || input.input.agent_instance_id != "root"
        {
            continue;
        }
        match active {
            Some(root) => assert_eq!(input.root_input_id.as_deref(), Some(root.as_str())),
            None => panic!("pending steer without active root"),
        }
    }
    if active.is_none() {
        assert!(snapshot.pending_steers.is_empty());
    } else {
        let mut steers: Vec<_> = aggregate
            .agent_inputs
            .values()
            .filter(|input| {
                input.input.agent_instance_id == "root"
                    && input.disposition == AgentInputDisposition::PendingSteer
            })
            .collect();
        steers.sort_by_key(|input| (input.admission_revision, input.input.input_id.clone()));
        assert_eq!(
            snapshot
                .pending_steers
                .iter()
                .map(|input| input.input_id.as_str())
                .collect::<Vec<_>>(),
            steers
                .iter()
                .map(|input| input.input.input_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    assert_eq!(
        snapshot.foreground,
        AgentForeground::project_work(
            snapshot.active_work.as_ref(),
            snapshot.pending_action.as_ref(),
            !snapshot.queued_inputs.is_empty(),
        )
    );
    assert_eq!(
        snapshot.pending_action.is_some(),
        snapshot.foreground == AgentForeground::RequiresAction
    );

    if let Some(active_work) = &snapshot.active_work {
        let cancelling = aggregate
            .interrupt_requested_roots
            .contains(&active_work.root_input_id);
        assert_eq!(
            active_work.state == piko_protocol::AgentWorkViewState::Cancelling,
            cancelling
        );
    }

    let mut seen = BTreeMap::new();
    for (input_id, stored) in &aggregate.agent_inputs {
        let previous = seen.insert(stored.input.request_id.clone(), input_id.clone());
        assert!(previous.is_none(), "request_id maps to one input");
        assert_eq!(
            aggregate.input_by_request.get(&stored.input.request_id),
            Some(input_id)
        );
    }
    assert_eq!(
        aggregate.input_by_request.len(),
        aggregate.agent_inputs.len()
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        ..ProptestConfig::default()
    })]

    #[test]
    fn agent_work_projection_invariants(ops in prop::collection::vec(0u8..8, 0..16)) {
        let temp = tempdir().unwrap();
        let opened = SessionStore::create(&temp.path().join("session"), new_session()).unwrap();
        let store = opened.store;
        let mut rev = 1u64;
        let mut next = 0u32;
        let mut active: Option<String> = None;
        let mut queued: Vec<String> = Vec::new();
        let mut steers: Vec<String> = Vec::new();
        let mut actions: Vec<String> = Vec::new();
        let mut interrupted = false;

        for op in ops {
            match op {
                0 if active.is_none() => {
                    let id = format!("root-{next}");
                    next += 1;
                    append_start(&store, rev, &id);
                    rev += 1;
                    active = Some(id);
                    interrupted = false;
                }
                1 => {
                    let id = format!("follow-{next}");
                    next += 1;
                    append(
                        &store,
                        rev,
                        &format!("admit-{id}"),
                        EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                            input: user_input(&id, piko_protocol::AgentInputDelivery::FollowUp),
                            disposition: AgentInputDisposition::PendingFollowUp,
                            root_input_id: None,
                            admitted_at: 2,
                        }),
                    );
                    rev += 1;
                    queued.push(id);
                }
                2 if active.is_some() => {
                    let id = format!("steer-{next}");
                    next += 1;
                    let root = active.clone().unwrap();
                    append(
                        &store,
                        rev,
                        &format!("admit-{id}"),
                        EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                            input: user_input(&id, piko_protocol::AgentInputDelivery::SteerActive),
                            disposition: AgentInputDisposition::PendingSteer,
                            root_input_id: Some(root),
                            admitted_at: 2,
                        }),
                    );
                    rev += 1;
                    steers.push(id);
                }
                3 if active.is_some() => {
                    let id = format!("act-{next}");
                    next += 1;
                    let root = active.clone().unwrap();
                    append(
                        &store,
                        rev,
                        &format!("req-{id}"),
                        EventData::AgentPendingActionRequestedV1(AgentPendingActionRequestedV1 {
                            agent_instance_id: "root".into(),
                            root_input_id: root,
                            action: PendingActionSummary {
                                action_id: id.clone(),
                                kind: "approval".into(),
                                summary: None,
                            },
                            requested_at: next as i64,
                        }),
                    );
                    rev += 1;
                    actions.push(id);
                }
                4 if !actions.is_empty() && active.is_some() => {
                    let id = actions.pop().unwrap();
                    let root = active.clone().unwrap();
                    append(
                        &store,
                        rev,
                        &format!("res-{id}"),
                        EventData::AgentPendingActionResolvedV1(AgentPendingActionResolvedV1 {
                            agent_instance_id: "root".into(),
                            root_input_id: root,
                            action_id: id,
                            resolved_at: 4,
                        }),
                    );
                    rev += 1;
                }
                5 if active.is_some() && !interrupted => {
                    let root = active.clone().unwrap();
                    append(
                        &store,
                        rev,
                        &format!("int-{root}"),
                        EventData::AgentInterruptRequestedV1(AgentInterruptRequestedV1 {
                            agent_instance_id: "root".into(),
                            root_input_id: root,
                            requested_at: 5,
                        }),
                    );
                    rev += 1;
                    interrupted = true;
                }
                6 if active.is_some() => {
                    let root = active.take().unwrap();
                    for id in steers.drain(..) {
                        append(
                            &store,
                            rev,
                            &format!("can-{id}"),
                            EventData::AgentInputDispositionChangedV1(
                                AgentInputDispositionChangedV1 {
                                    agent_instance_id: "root".into(),
                                    input_id: id,
                                    disposition: AgentInputDisposition::Cancelled,
                                    root_input_id: Some(root.clone()),
                                    model_step_id: None,
                                    changed_at: 6,
                                },
                            ),
                        );
                        rev += 1;
                    }
                    append(
                        &store,
                        rev,
                        &format!("fin-{root}"),
                        EventData::AgentInputProcessingFinishedV1(AgentInputProcessingFinishedV1 {
                            agent_instance_id: "root".into(),
                            root_input_id: root.clone(),
                            report: report(&root),
                            finished_at: 6,
                        }),
                    );
                    rev += 1;
                    actions.clear();
                    interrupted = false;
                }
                7 if !queued.is_empty() => {
                    let id = queued.pop().unwrap();
                    append(
                        &store,
                        rev,
                        &format!("can-{id}"),
                        EventData::AgentInputDispositionChangedV1(AgentInputDispositionChangedV1 {
                            agent_instance_id: "root".into(),
                            input_id: id,
                            disposition: AgentInputDisposition::Cancelled,
                            root_input_id: None,
                            model_step_id: None,
                            changed_at: 7,
                        }),
                    );
                    rev += 1;
                }
                _ => {}
            }
            assert_invariants(&store);
        }
    }
}
