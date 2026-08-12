use piko_hostd::HostState;
use piko_hostd::api::ServerMessage as Event;

#[test]
fn start_turn_queues_second_turn_for_same_agent() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "first")
        .unwrap();
    let first_status = state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let (queued_turn_id, second_status) = state
        .start_turn(&session_id, &agent_instance_id, "second")
        .unwrap();
    assert_eq!(first_status, piko_hostd::api::TurnStatus::Running);
    assert_eq!(second_status, piko_hostd::api::TurnStatus::Queued);

    state.complete_turn(&session_id, &turn_id).unwrap();
    state
        .mark_turn_running(&session_id, &queued_turn_id)
        .unwrap();
    assert_eq!(
        state.turn(&session_id, &queued_turn_id).unwrap().status,
        piko_hostd::api::TurnStatus::Running
    );
}

#[test]
fn different_agents_can_own_running_turns_concurrently() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let root_agent_instance_id = format!("agent_{session_id}_root");
    let (root_turn_id, _) = state
        .start_turn(&session_id, &root_agent_instance_id, "root input")
        .unwrap();
    let root_status = state
        .apply_turn_input_disposition(
            &session_id,
            &root_turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let (child_turn_id, _) = state
        .start_turn(&session_id, "agent-child", "child input")
        .unwrap();
    let child_status = state
        .apply_turn_input_disposition(
            &session_id,
            &child_turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();

    assert_eq!(root_status, piko_hostd::api::TurnStatus::Running);
    assert_eq!(child_status, piko_hostd::api::TurnStatus::Running);
    assert_eq!(state.snapshot(&session_id).unwrap().active_turns.len(), 2);
}

#[test]
fn create_session_emits_session_created() {
    let mut state = HostState::new();
    let event = state.create_session("/tmp/project");
    assert!(matches!(
        event,
        piko_hostd::api::CommandResult::SessionCreated { .. }
    ));
}

#[test]
fn can_start_and_complete_turn() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "hello")
        .unwrap();
    let status = state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    assert_eq!(status, piko_hostd::api::TurnStatus::Running);

    let complete = state.complete_turn(&session_id, &turn_id).unwrap();
    assert!(matches!(
        &complete,
        Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed { .. })
    ));
    let replay = state.complete_turn(&session_id, &turn_id).unwrap();
    let Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed {
        session_id: s1,
        turn_id: t1,
        ..
    }) = complete
    else {
        panic!("expected Completed turn event");
    };
    let Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed {
        session_id: s2,
        turn_id: t2,
        ..
    }) = replay
    else {
        panic!("expected Completed turn event");
    };
    assert_eq!(s1, s2);
    assert_eq!(t1, t2);
}

#[test]
fn fail_turn_emits_turn_failed() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "fail")
        .unwrap();
    let fail = state
        .fail_turn(&session_id, &turn_id, "test error")
        .unwrap();
    assert!(matches!(
        fail,
        Event::TurnLifecycle(piko_hostd::api::TurnEvent::Failed { .. })
    ));
}

#[test]
fn cancel_turn_emits_turn_cancelled() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "cancel")
        .unwrap();
    let cancel = state.cancel_turn(&session_id, &turn_id).unwrap();
    assert!(matches!(
        cancel,
        Event::TurnLifecycle(piko_hostd::api::TurnEvent::Cancelled { .. })
    ));
}

#[test]
fn finalize_interrupted_turns_clears_active_turn_and_emits_failed() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "interrupt")
        .unwrap();
    state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let events = state.finalize_interrupted_turns(&session_id).unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_hostd::api::TurnEvent::Failed {
            turn_id: failed_id,
            error,
            ..
        }) if failed_id == &turn_id && error.contains("interrupted")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Usage(piko_hostd::api::UsageEvent::Updated { .. })
    )));

    let snapshot = state.snapshot(&session_id).unwrap();
    assert!(snapshot.active_turns.is_empty());

    // Idempotent when no active turn remains.
    assert!(
        state
            .finalize_interrupted_turns(&session_id)
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .start_turn(&session_id, &agent_instance_id, "again")
            .is_ok()
    );
}

#[test]
fn multi_step_usages_roll_up_on_turn_completed() {
    use piko_protocol::{
        ContentBlock, Message, MessageEntry, SessionTreeEntry,
        messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry},
    };

    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "hello")
        .unwrap();
    state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();

    let step = |input: u64, output: u64| Usage {
        input,
        output,
        cache_read: 0,
        cache_write: 0,
        total_tokens: input + output,
        units: Default::default(),
        cost: UsageCost {
            entries: vec![UsageCostEntry {
                currency: "USD".into(),
                basis: UsageCostBasis::ListPrice,
                components: [
                    ("input_tokens".into(), input as f64 * 0.001),
                    ("output_tokens".into(), output as f64 * 0.002),
                ]
                .into(),
                total: input as f64 * 0.001 + output as f64 * 0.002,
            }],
        },
    };

    for (i, usage) in [step(10, 5), step(20, 7)].into_iter().enumerate() {
        let entry = SessionTreeEntry::Message(MessageEntry {
            id: format!("asst-{i}"),
            parent_id: None,
            timestamp: format!("{i}"),
            agent_id: "main".into(),
            agent_instance_id: agent_instance_id.clone(),
            source_turn_id: turn_id.clone(),
            transcript_seq: (i as u64) + 1,
            message: Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: format!("step {i}"),
                }],
                checkpoint: None,
                provider: "test".into(),
                model: "test-model".into(),
                usage: Some(usage.clone()),
                stop_reason: Some("stop".into()),
                error_message: None,
                timestamp: Some(i as i64),
            },
        });
        state
            .session_mut(&session_id)
            .unwrap()
            .account_step_usage(Some(&turn_id), &usage);
        state.session_mut(&session_id).unwrap().entries.push(entry);
    }

    let turn_usage = state.turn(&session_id, &turn_id).unwrap().usage.clone();
    assert_eq!(turn_usage.input, 30);
    assert_eq!(turn_usage.output, 12);
    assert_eq!(turn_usage.total_tokens, 42);

    let cumulative = state.session(&session_id).unwrap().cumulative_usage.clone();
    assert_eq!(cumulative.input, 30);
    assert_eq!(cumulative.output, 12);

    let Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed { usage, .. }) =
        state.complete_turn(&session_id, &turn_id).unwrap()
    else {
        panic!("expected Completed");
    };
    assert_eq!(usage.input, 30);
    assert_eq!(usage.output, 12);
    assert_eq!(usage.total_tokens, 42);

    let agent_usage = &state.session(&session_id).unwrap().agent_usage;
    assert_eq!(agent_usage[&agent_instance_id].input, 30);
    assert_eq!(agent_usage[&agent_instance_id].output, 12);
}

#[test]
fn step_usage_accounts_into_turn_and_session() {
    use piko_protocol::messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "hi")
        .unwrap();

    let usage = Usage {
        input: 11,
        output: 3,
        cache_read: 2,
        cache_write: 1,
        total_tokens: 17,
        units: Default::default(),
        cost: UsageCost {
            entries: vec![UsageCostEntry {
                currency: "USD".into(),
                basis: UsageCostBasis::ListPrice,
                components: [
                    ("input_tokens".into(), 0.01),
                    ("output_tokens".into(), 0.02),
                    ("cached_input_tokens".into(), 0.001),
                    ("cache_write_tokens".into(), 0.002),
                ]
                .into(),
                total: 0.033,
            }],
        },
    };
    state
        .session_mut(&session_id)
        .unwrap()
        .account_step_usage(Some(&turn_id), &usage);

    assert_eq!(
        state
            .turn(&session_id, &turn_id)
            .unwrap()
            .usage
            .total_tokens,
        17
    );
    assert_eq!(
        state
            .session(&session_id)
            .unwrap()
            .cumulative_usage
            .total_tokens,
        17
    );
}
