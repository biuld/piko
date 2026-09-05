use std::collections::BTreeMap;

use piko_protocol::{
    AgentInput, AgentInputDelivery, AgentInputDisposition, AgentInputOrigin, Message,
    MessageContent, ModelStepOutcome,
};

use super::history::{HistoryProjection, HistoryProvenance, apply_commit};
use crate::journal::{Checksum, Producer};
use crate::{
    AgentInputAdmittedV1, AgentOriginRecordedV1, DurableCommit, EventData, MessageCommittedV1,
    ModelStepCommittedV1, RawEvent, SCHEMA_VERSION,
};

fn commit(revision: u64, events: Vec<RawEvent>) -> DurableCommit {
    DurableCommit {
        schema_version: SCHEMA_VERSION,
        session_id: "s1".into(),
        journal_generation: "g1".into(),
        revision,
        commit_id: format!("commit-{revision}"),
        committed_at: revision as i64,
        producer: Producer {
            component: "test".into(),
            version: "1".into(),
        },
        causation_id: None,
        correlation_id: None,
        previous_checksum: None,
        events,
        extensions: BTreeMap::new(),
        checksum: Checksum {
            algorithm: "crc32".into(),
            value: format!("sum-{revision}"),
        },
    }
}

fn event(id: &str, data: EventData) -> RawEvent {
    RawEvent::new(id, data).unwrap()
}

fn message(id: &str, message: Message) -> RawEvent {
    event(
        id,
        EventData::MessageCommitted(MessageCommittedV1 {
            message_id: id.into(),
            agent_instance_id: "root".into(),
            agent_parent_message_id: None,
            tree_parent_entry_id: None,
            root_input_id: Some("input-1".into()),
            committed_at: 2,
            message,
        }),
    )
}

#[test]
fn projection_preserves_commit_order_and_builds_step_indexes() {
    let mut projection = HistoryProjection::default();
    apply_commit(
        &mut projection,
        &commit(
            1,
            vec![event(
                "admitted",
                EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                    input: AgentInput {
                        input_id: "input-1".into(),
                        request_id: "request-1".into(),
                        session_id: "s1".into(),
                        agent_instance_id: "root".into(),
                        origin: AgentInputOrigin::User,
                        delivery: AgentInputDelivery::StartWhenIdle,
                        content: MessageContent::String("hello".into()),
                        submitted_at: 1,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: AgentInputDisposition::AppliedAsRoot,
                    root_input_id: Some("input-1".into()),
                    admitted_at: 1,
                }),
            )],
        ),
    );
    apply_commit(
        &mut projection,
        &commit(
            2,
            vec![
                message(
                    "assistant-1",
                    Message::Assistant {
                        content: Vec::new(),
                        checkpoint: None,
                        provider: "openai".into(),
                        model: "gpt".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(2),
                    },
                ),
                message(
                    "tool-message-1",
                    Message::ToolCall {
                        id: "call-1".into(),
                        name: "read".into(),
                        arguments: serde_json::json!({"path": "README.md"}),
                        model: Some("gpt".into()),
                        provider: Some("openai".into()),
                        timestamp: Some(2),
                    },
                ),
                event(
                    "step-1",
                    EventData::ModelStepCommitted(ModelStepCommittedV1 {
                        model_step_id: "step-1".into(),
                        step_index: 1,
                        root_input_id: "input-1".into(),
                        agent_instance_id: "root".into(),
                        assistant_message_id: "assistant-1".into(),
                        tool_call_message_ids: vec!["tool-message-1".into()],
                        outcome: ModelStepOutcome::ToolCalls,
                        started_at: 1,
                        finished_at: 2,
                    }),
                ),
            ],
        ),
    );
    apply_commit(
        &mut projection,
        &commit(
            3,
            vec![event(
                "child-origin",
                EventData::AgentOriginRecordedV1(AgentOriginRecordedV1 {
                    child_agent_instance_id: "child".into(),
                    parent_agent_instance_id: "root".into(),
                    parent_root_input_id: "input-1".into(),
                    origin_model_step_id: "step-1".into(),
                    origin_tool_call_id: "call-1".into(),
                    recorded_at: 3,
                }),
            )],
        ),
    );

    assert_eq!(projection.revision, 3);
    assert_eq!(projection.commits.len(), 3);
    assert_eq!(projection.commits[0].revision, 1);
    assert_eq!(projection.commits[1].events[2].event_id, "step-1");
    assert_eq!(
        projection.commits[1].events[0].model_step_id.as_deref(),
        Some("step-1")
    );
    assert_eq!(
        projection.commits[1].events[1].model_step_id.as_deref(),
        Some("step-1")
    );
    assert_eq!(projection.work_commit_indexes["input-1"], vec![0, 1, 2]);
    assert_eq!(projection.agent_commit_indexes["root"], vec![0, 1]);
    assert_eq!(projection.agent_commit_indexes["child"], vec![2]);
    assert_eq!(projection.message_to_step["assistant-1"], "step-1");
    assert_eq!(projection.message_to_step["tool-message-1"], "step-1");
    assert_eq!(projection.tool_call_to_step["call-1"], "step-1");
    assert_eq!(
        projection.child_origins["child"].origin_model_step_id,
        "step-1"
    );
}

#[test]
fn optional_observation_is_diagnostic_and_never_creates_fact_relations() {
    let mut projection = HistoryProjection::default();
    let observation = RawEvent::optional(
        "trajectory-step",
        "trajectory.model_step",
        serde_json::json!({
            "identity": {
                "sessionId": "s1",
                "agentInstanceId": "root",
                "rootInputId": "input-1"
            },
            "stepId": "step-1"
        }),
    );
    apply_commit(&mut projection, &commit(1, vec![observation]));

    let event = &projection.commits[0].events[0];
    assert_eq!(event.provenance, HistoryProvenance::Diagnostic);
    assert_eq!(event.root_input_id.as_deref(), Some("input-1"));
    assert_eq!(event.model_step_id.as_deref(), Some("step-1"));
    assert!(projection.message_to_step.is_empty());
    assert!(projection.tool_call_to_step.is_empty());
    assert!(projection.work_commit_indexes.is_empty());
    assert!(projection.agent_commit_indexes.is_empty());
}

#[test]
fn unknown_optional_event_cannot_populate_causal_indexes() {
    let mut projection = HistoryProjection::default();
    apply_commit(
        &mut projection,
        &commit(
            1,
            vec![RawEvent::optional(
                "future",
                "future.observation",
                serde_json::json!({ "identity": { "agentInstanceId": "root", "rootInputId": "input-1" }, "stepId": "step-1" }),
            )],
        ),
    );
    assert!(projection.work_commit_indexes.is_empty());
    assert!(projection.agent_commit_indexes.is_empty());
    assert!(projection.message_to_step.is_empty());
    assert_eq!(
        projection.commits[0].events[0].event_type,
        "future.observation"
    );
    assert_eq!(
        projection.commits[0].events[0].provenance,
        HistoryProvenance::Diagnostic
    );
}

#[test]
fn usage_corrections_keep_the_original_work_relation_and_replacement() {
    use super::history::HistoryTransition;
    use crate::{UsageAttribution, UsageCorrectedV1, UsageRecordedV1};
    let mut projection = HistoryProjection::default();
    apply_commit(
        &mut projection,
        &commit(
            1,
            vec![event(
                "usage",
                EventData::UsageRecorded(UsageRecordedV1 {
                    usage_id: "usage-1".into(),
                    attribution: UsageAttribution {
                        session_id: "s1".into(),
                        agent_instance_id: "root".into(),
                        root_input_id: "input-1".into(),
                        model_step_id: "step-1".into(),
                    },
                    provider: "test".into(),
                    model_id: "model".into(),
                    api_surface: None,
                    pricing_policy_id: None,
                    pricing_revision: None,
                    usage: Default::default(),
                    incurred: true,
                }),
            )],
        ),
    );
    let replacement = piko_protocol::Usage {
        input: 123,
        ..Default::default()
    };
    apply_commit(
        &mut projection,
        &commit(
            2,
            vec![event(
                "correction",
                EventData::UsageCorrected(UsageCorrectedV1 {
                    correction_id: "correction-1".into(),
                    usage_id: "usage-1".into(),
                    replacement: replacement.clone(),
                    reason: "provider correction".into(),
                }),
            )],
        ),
    );
    assert_eq!(projection.work_commit_indexes["input-1"], vec![0, 1]);
    let correction = &projection.commits[1].events[0];
    assert_eq!(correction.model_step_id.as_deref(), Some("step-1"));
    assert!(
        matches!(&correction.transition, Some(HistoryTransition::UsageCorrected { replacement: value, .. }) if value == &replacement)
    );
}

#[test]
fn incremental_history_equals_replaying_the_same_commits() {
    let commits = vec![
        commit(
            1,
            vec![event(
                "admitted",
                EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                    input: AgentInput {
                        input_id: "input-1".into(),
                        request_id: "request-1".into(),
                        session_id: "s1".into(),
                        agent_instance_id: "root".into(),
                        origin: AgentInputOrigin::User,
                        delivery: AgentInputDelivery::StartWhenIdle,
                        content: MessageContent::String("hello".into()),
                        submitted_at: 1,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: AgentInputDisposition::AppliedAsRoot,
                    root_input_id: Some("input-1".into()),
                    admitted_at: 1,
                }),
            )],
        ),
        commit(
            2,
            vec![message(
                "assistant-1",
                Message::Assistant {
                    content: Vec::new(),
                    checkpoint: None,
                    provider: "openai".into(),
                    model: "gpt".into(),
                    usage: None,
                    stop_reason: None,
                    error_message: None,
                    timestamp: Some(2),
                },
            )],
        ),
        commit(
            3,
            vec![RawEvent::optional(
                "assembly",
                "trajectory.assembly",
                serde_json::json!({
                    "identity": {
                        "sessionId": "s1",
                        "agentInstanceId": "root",
                        "rootInputId": "input-1"
                    }
                }),
            )],
        ),
    ];
    let mut incremental = HistoryProjection::default();
    for item in &commits {
        apply_commit(&mut incremental, item);
    }
    let mut replayed = HistoryProjection::default();
    for item in &commits {
        apply_commit(&mut replayed, item);
    }
    assert_eq!(incremental, replayed);
}

#[test]
fn optional_events_do_not_change_fact_indexes() {
    let fact = commit(
        1,
        vec![event(
            "admitted",
            EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                input: AgentInput {
                    input_id: "input-1".into(),
                    request_id: "request-1".into(),
                    session_id: "s1".into(),
                    agent_instance_id: "root".into(),
                    origin: AgentInputOrigin::User,
                    delivery: AgentInputDelivery::StartWhenIdle,
                    content: MessageContent::String("hello".into()),
                    submitted_at: 1,
                    caller_agent_instance_id: None,
                    detached_recipient_agent_instance_id: None,
                },
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: Some("input-1".into()),
                admitted_at: 1,
            }),
        )],
    );
    let diagnostic = commit(
        2,
        vec![RawEvent::optional(
            "assembly",
            "trajectory.assembly",
            serde_json::json!({
                "identity": {
                    "sessionId": "s1",
                    "agentInstanceId": "root",
                    "rootInputId": "input-1"
                }
            }),
        )],
    );
    let mut facts_only = HistoryProjection::default();
    apply_commit(&mut facts_only, &fact);
    let mut with_diagnostics = facts_only.clone();
    apply_commit(&mut with_diagnostics, &diagnostic);
    assert_eq!(
        facts_only.work_commit_indexes,
        with_diagnostics.work_commit_indexes
    );
    assert_eq!(
        facts_only.agent_commit_indexes,
        with_diagnostics.agent_commit_indexes
    );
    assert_eq!(facts_only.message_to_step, with_diagnostics.message_to_step);
    assert_eq!(
        facts_only.tool_call_to_step,
        with_diagnostics.tool_call_to_step
    );
    assert_eq!(with_diagnostics.commits.len(), 2);
    assert_eq!(
        with_diagnostics.commits[1].events[0].provenance,
        HistoryProvenance::Diagnostic
    );
}
