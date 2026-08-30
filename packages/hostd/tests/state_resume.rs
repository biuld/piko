use piko_hostd::HostState;

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
fn multi_step_usages_roll_up_on_agent_and_session() {
    use piko_protocol::messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let agent_instance_id = format!("agent_{session_id}_root");

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

    for usage in [step(10, 5), step(20, 7)] {
        state
            .session_mut(&session_id)
            .unwrap()
            .account_step_usage(Some(&agent_instance_id), &usage);
    }

    let cumulative = state.session(&session_id).unwrap().cumulative_usage.clone();
    assert_eq!(cumulative.input, 30);
    assert_eq!(cumulative.output, 12);

    let agent_usage = &state.session(&session_id).unwrap().agent_usage;
    assert_eq!(agent_usage[&agent_instance_id].input, 30);
    assert_eq!(agent_usage[&agent_instance_id].output, 12);
}

#[test]
fn step_usage_accounts_into_agent_and_session() {
    use piko_protocol::messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        piko_hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let agent_instance_id = format!("agent_{session_id}_root");

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
        .account_step_usage(Some(&agent_instance_id), &usage);

    assert_eq!(
        state.session(&session_id).unwrap().agent_usage[&agent_instance_id].total_tokens,
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
