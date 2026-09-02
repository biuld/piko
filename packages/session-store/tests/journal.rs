use std::collections::BTreeMap;
use std::fs::OpenOptions as FsOpenOptions;
use std::io::Write;

use piko_protocol::{
    AgentInstanceIdentity, ContentBlock, Message, MessageContent, ModelStepOutcome, Usage,
};
use piko_session_store::{
    AgentInputAdmittedV1, AgentInputAppliedV1, AgentInputProcessingStartedV1, CompactionRecordedV1,
    EventData, MessageCommittedV1, ModelStepCommittedV1, NewSession, OpenOptions, ProposedCommit,
    RawEvent, SessionStore, StoreError, UsageAttribution, UsageCorrectedV1, UsageQuery,
    UsageRecordedV1,
};
use tempfile::tempdir;

fn root_input(id: &str, request_id: &str) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: id.into(),
        request_id: request_id.into(),
        session_id: "s1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
        content: MessageContent::String("work".into()),
        submitted_at: 2,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}

/// Admit an applied root input and start its processing in one commit.
fn work_start(
    id: &str,
    request_id: &str,
    _execution_id: &str,
    _run_id: &str,
    turn_id: &str,
) -> ProposedCommit {
    let input = root_input(id, request_id);
    ProposedCommit {
        commit_id: format!("work-start-{id}"),
        committed_at: 2,
        causation_id: None,
        correlation_id: Some(turn_id.into()),
        events: vec![
            event(
                "input-admitted",
                EventData::AgentInputAdmittedV1(AgentInputAdmittedV1 {
                    input,
                    disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
                    root_input_id: None,
                    admitted_at: 2,
                }),
            ),
            event(
                "processing-started",
                EventData::AgentInputProcessingStartedV1(AgentInputProcessingStartedV1 {
                    agent_instance_id: "root".into(),
                    root_input_id: id.into(),
                    request_id: request_id.into(),
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

fn commit(id: &str, at: i64, event: RawEvent) -> ProposedCommit {
    ProposedCommit::one(id, at, event)
}

fn message(id: &str, agent_parent: Option<&str>, tree_parent: Option<&str>) -> EventData {
    EventData::MessageCommitted(MessageCommittedV1 {
        message_id: id.into(),
        agent_instance_id: "root".into(),
        agent_parent_message_id: agent_parent.map(str::to_string),
        tree_parent_entry_id: tree_parent.map(str::to_string),
        root_input_id: Some(format!("input-{id}")),
        committed_at: 2,
        message: Message::User {
            content: MessageContent::String(id.into()),
            timestamp: Some(2),
        },
    })
}

fn model_step_messages() -> (EventData, EventData, EventData) {
    let assistant = EventData::MessageCommitted(MessageCommittedV1 {
        message_id: "assistant-1".into(),
        agent_instance_id: "root".into(),
        agent_parent_message_id: None,
        tree_parent_entry_id: None,
        root_input_id: Some("input-1".into()),
        committed_at: 3,
        message: Message::Assistant {
            content: vec![
                ContentBlock::Thinking {
                    thinking: "inspect".into(),
                    thinking_signature: None,
                    duration_ms: Some(7),
                },
                ContentBlock::Text {
                    text: "I will inspect this".into(),
                },
            ],
            checkpoint: None,
            provider: "test".into(),
            model: "model".into(),
            usage: None,
            stop_reason: Some("tool_calls".into()),
            error_message: None,
            timestamp: Some(3),
        },
    });
    let tool_call = EventData::MessageCommitted(MessageCommittedV1 {
        message_id: "tool-call-message-1".into(),
        agent_instance_id: "root".into(),
        agent_parent_message_id: Some("assistant-1".into()),
        tree_parent_entry_id: Some("assistant-1".into()),
        root_input_id: Some("input-1".into()),
        committed_at: 3,
        message: Message::ToolCall {
            id: "call-1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "README.md"}),
            model: None,
            provider: None,
            timestamp: Some(3),
        },
    });
    let boundary = EventData::ModelStepCommitted(ModelStepCommittedV1 {
        model_step_id: "step-1".into(),
        step_index: 1,
        root_input_id: "input-1".into(),
        agent_instance_id: "root".into(),
        assistant_message_id: "assistant-1".into(),
        tool_call_message_ids: vec!["tool-call-message-1".into()],
        outcome: ModelStepOutcome::ToolCalls,
        started_at: 2,
        finished_at: 3,
    });
    (assistant, tool_call, boundary)
}

#[test]
fn model_step_commit_replays_as_an_atomic_authoritative_relation() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(
            1,
            work_start("input-1", "turn-1", "execution-1", "run-1", "turn-1"),
        )
        .unwrap();
    let (assistant, tool_call, boundary) = model_step_messages();
    let proposed = ProposedCommit {
        commit_id: "model-step-commit".into(),
        committed_at: 3,
        causation_id: None,
        correlation_id: Some("turn-1".into()),
        events: vec![
            event("assistant-event", assistant),
            event("tool-call-event", tool_call),
            event("model-step-event", boundary),
        ],
        extensions: BTreeMap::new(),
    };
    let first = opened.store.append(2, proposed.clone()).unwrap();
    let retry = opened.store.append(2, proposed).unwrap();
    assert_eq!(first, retry);

    let aggregate = opened.store.aggregate();
    let processing = aggregate.agent_inputs["input-1"]
        .processing
        .as_ref()
        .unwrap();
    assert_eq!(processing.started_at, 2);
    assert_eq!(
        aggregate.model_steps["step-1"].data.root_input_id,
        "input-1"
    );
    assert_eq!(
        aggregate.model_steps["step-1"].data.tool_call_message_ids,
        ["tool-call-message-1"]
    );
    assert_eq!(aggregate.revision, 3);

    drop(opened);
    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(
        reopened.aggregate.model_steps["step-1"].data.outcome,
        ModelStepOutcome::ToolCalls
    );
}

#[test]
fn model_step_boundary_rejects_messages_from_an_earlier_revision() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    opened
        .store
        .append(
            1,
            work_start("input-1", "turn-1", "execution-1", "run-1", "turn-1"),
        )
        .unwrap();
    let (assistant, tool_call, boundary) = model_step_messages();
    opened
        .store
        .append(
            2,
            ProposedCommit {
                commit_id: "messages-without-boundary".into(),
                committed_at: 3,
                causation_id: None,
                correlation_id: Some("turn-1".into()),
                events: vec![
                    event("assistant-event", assistant),
                    event("tool-call-event", tool_call),
                ],
                extensions: BTreeMap::new(),
            },
        )
        .unwrap();

    let error = opened
        .store
        .append(
            3,
            commit(
                "late-model-step-boundary",
                4,
                event("model-step-event", boundary),
            ),
        )
        .expect_err("a boundary cannot retroactively make earlier messages atomic");

    assert!(matches!(error, StoreError::InvalidEvent(message) if message.contains("revision")));
    assert_eq!(opened.store.aggregate().revision, 3);
    assert!(opened.store.aggregate().model_steps.is_empty());
}

#[test]
fn admitted_applied_to_step_is_rejected_without_panicking() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let input = piko_protocol::AgentInput {
        input_id: "input-step-admission".into(),
        request_id: "request-step-admission".into(),
        session_id: "s1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::SteerActive,
        content: MessageContent::String("invalid admission".into()),
        submitted_at: 2,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let error = opened
        .store
        .append(
            1,
            commit(
                "invalid-step-admission",
                2,
                event(
                    "invalid-step-admission-event",
                    EventData::AgentInputAdmittedV1(piko_session_store::AgentInputAdmittedV1 {
                        input,
                        disposition: piko_protocol::AgentInputDisposition::AppliedToStep,
                        root_input_id: Some("root-input".into()),
                        admitted_at: 2,
                    }),
                ),
            ),
        )
        .expect_err("applied_to_step is a transition, not an admission");
    assert!(matches!(
        error,
        piko_session_store::StoreError::InvalidEvent(message)
            if message.contains("applied_to_step")
    ));
    assert_eq!(opened.store.aggregate().revision, 1);
}

#[test]
fn applied_agent_input_is_the_single_durable_user_payload_authority() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let input = piko_protocol::AgentInput {
        input_id: "input-1".into(),
        request_id: "request-1".into(),
        session_id: "s1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
        content: MessageContent::String("only-payload-copy".into()),
        submitted_at: 2,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let admitted = event(
        "input-admitted",
        EventData::AgentInputAdmittedV1(piko_session_store::AgentInputAdmittedV1 {
            input,
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            root_input_id: Some("input-1".into()),
            admitted_at: 2,
        }),
    );
    let started = event(
        "processing-started",
        EventData::AgentInputProcessingStartedV1(AgentInputProcessingStartedV1 {
            agent_instance_id: "root".into(),
            root_input_id: "input-1".into(),
            request_id: "request-1".into(),
            base_message_id: None,
            tree_base_entry_id: None,
            detached_recipient_agent_instance_id: None,
            prompt_assembly_version: 1,
            prompt_digest: "digest".into(),
            started_at: 2,
        }),
    );
    let applied = event(
        "input-applied",
        EventData::AgentInputAppliedV1(AgentInputAppliedV1 {
            input_id: "input-1".into(),
            message_id: "message-1".into(),
            agent_instance_id: "root".into(),
            agent_parent_message_id: None,
            tree_parent_entry_id: None,
            root_input_id: "input-1".into(),
            committed_at: 3,
        }),
    );
    let durable_json = format!(
        "{}{}{}",
        serde_json::to_string(&admitted).unwrap(),
        serde_json::to_string(&started).unwrap(),
        serde_json::to_string(&applied).unwrap()
    );
    assert_eq!(durable_json.matches("only-payload-copy").count(), 1);

    opened
        .store
        .append(
            1,
            ProposedCommit {
                commit_id: "start".into(),
                committed_at: 2,
                causation_id: None,
                correlation_id: Some("turn-1".into()),
                events: vec![admitted, started],
                extensions: BTreeMap::new(),
            },
        )
        .unwrap();
    opened.store.append(2, commit("apply", 3, applied)).unwrap();
    let aggregate = opened.store.aggregate();
    let message = &aggregate.messages["message-1"].data.message;
    assert!(matches!(
        message,
        Message::User { content: MessageContent::String(text), .. }
            if text == "only-payload-copy"
    ));
    assert_eq!(
        aggregate.agent_inputs["input-1"]
            .applied_message_id
            .as_deref(),
        Some("message-1")
    );
}

#[test]
fn append_reopen_and_idempotent_retry_converge() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let proposed = commit("c2", 2, event("e2", message("m1", None, None)));
    let first = opened.store.append(1, proposed.clone()).unwrap();
    let retry = opened.store.append(1, proposed).unwrap();
    assert_eq!(first, retry);
    assert_eq!(opened.store.aggregate().revision, 2);
    drop(opened);

    let reopened =
        SessionStore::open(&temp.path().join("session"), OpenOptions::default()).unwrap();
    assert_eq!(reopened.aggregate.revision, 2);
    assert_eq!(
        reopened.aggregate.active_root_transcript().unwrap().len(),
        1
    );
}

#[test]
fn trajectory_read_model_is_published_and_survives_reopen() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    let optional = RawEvent::optional(
        "opt-1",
        "trajectory.assembly",
        serde_json::json!({"runId": "r1"}),
    );
    opened.store.append(2, commit("c3", 3, optional)).unwrap();
    assert_eq!(opened.store.trajectory().revision, 3);
    drop(opened);

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.store.trajectory().revision, 3);
}

#[test]
fn same_process_open_reuses_the_single_writer_core() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let first = SessionStore::create(&path, new_session("s1")).unwrap();
    let second = SessionStore::open(&path, OpenOptions::default()).unwrap();

    first
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    assert_eq!(second.store.aggregate().revision, 2);
}

#[test]
fn writer_lock_child_process() {
    let Ok(path) = std::env::var("PIKO_WRITER_LOCK_TEST_PATH") else {
        return;
    };
    let error =
        SessionStore::open(std::path::Path::new(&path), OpenOptions::default()).unwrap_err();
    assert!(matches!(error, StoreError::WriterLocked(_)));
}

#[test]
fn filesystem_writer_lock_rejects_another_process() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("writer_lock_child_process")
        .arg("--nocapture")
        .env("PIKO_WRITER_LOCK_TEST_PATH", &path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    drop(opened);
}

#[test]
fn stale_revision_is_rejected_without_appending() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    let error = opened
        .store
        .append(
            1,
            commit("stale", 3, event("stale-event", message("m2", None, None))),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        StoreError::RevisionConflict {
            expected: 1,
            current: 2
        }
    ));
    assert_eq!(opened.store.aggregate().revision, 2);
}

#[test]
fn conflicting_retry_is_rejected() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("same", 2, event("e2", message("m1", None, None))))
        .unwrap();
    let error = opened
        .store
        .append(1, commit("same", 3, event("e3", message("m2", None, None))))
        .unwrap_err();
    assert!(matches!(error, StoreError::IdempotencyConflict(id) if id == "same"));
}

#[test]
fn selected_branch_controls_active_transcript() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let store = &opened.store;
    store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    store
        .append(
            2,
            commit("c3", 3, event("e3", message("m2", Some("m1"), Some("m1")))),
        )
        .unwrap();
    store
        .append(
            3,
            commit(
                "c4",
                4,
                event(
                    "e4",
                    EventData::BranchSelected {
                        selected_tree_entry_id: Some("m1".into()),
                        root_base_message_id: Some("m1".into()),
                    },
                ),
            ),
        )
        .unwrap();
    store
        .append(
            4,
            commit("c5", 5, event("e5", message("m3", Some("m1"), Some("m1")))),
        )
        .unwrap();
    store
        .append(
            5,
            commit(
                "c6",
                6,
                event(
                    "e6",
                    EventData::BranchSelected {
                        selected_tree_entry_id: Some("m3".into()),
                        root_base_message_id: Some("m3".into()),
                    },
                ),
            ),
        )
        .unwrap();

    let transcript = store.aggregate().active_root_transcript().unwrap();
    let texts: Vec<&str> = transcript
        .iter()
        .filter_map(|message| match message {
            Message::User {
                content: MessageContent::String(text),
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["m1", "m3"]);
}

#[test]
fn usage_correction_is_replayed_exactly_once() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let usage = |input| Usage {
        input,
        total_tokens: input,
        ..Usage::default()
    };
    let fact = UsageRecordedV1 {
        usage_id: "u1".into(),
        attribution: UsageAttribution {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            root_input_id: "input-1".into(),
            model_step_id: "step1".into(),
        },
        provider: "test".into(),
        model_id: "model".into(),
        api_surface: None,
        pricing_policy_id: None,
        pricing_revision: None,
        usage: usage(10),
        incurred: true,
    };
    opened
        .store
        .append(
            1,
            commit("c2", 2, event("e2", EventData::UsageRecorded(fact))),
        )
        .unwrap();
    opened
        .store
        .append(
            2,
            commit(
                "c3",
                3,
                event(
                    "e3",
                    EventData::UsageCorrected(UsageCorrectedV1 {
                        correction_id: "fix1".into(),
                        usage_id: "u1".into(),
                        replacement: usage(12),
                        reason: "provider final".into(),
                    }),
                ),
            ),
        )
        .unwrap();
    let aggregate = opened.store.aggregate();
    let summary = aggregate.accounting.summarize_incurred();
    assert_eq!(summary.usage.input, 12);
    assert_eq!(
        aggregate.accounting.summarize(&UsageQuery {
            root_input_id: Some("input-1".into()),
            provider: Some("test".into()),
            incurred_only: true,
            ..UsageQuery::default()
        }),
        summary
    );
}

#[test]
fn usage_is_stable_across_retry_navigation_compaction_snapshot_and_replay() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(
            1,
            commit("message", 2, event("em", message("m1", None, None))),
        )
        .unwrap();
    let fact = UsageRecordedV1 {
        usage_id: "usage-once".into(),
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
        usage: Usage {
            input: 10,
            output: 4,
            total_tokens: 14,
            ..Usage::default()
        },
        incurred: true,
    };
    let usage_commit = commit("usage", 3, event("eu", EventData::UsageRecorded(fact)));
    opened.store.append(2, usage_commit.clone()).unwrap();
    opened.store.append(2, usage_commit).unwrap();
    opened
        .store
        .append(
            3,
            commit(
                "navigate",
                4,
                event(
                    "en",
                    EventData::BranchSelected {
                        selected_tree_entry_id: Some("m1".into()),
                        root_base_message_id: Some("m1".into()),
                    },
                ),
            ),
        )
        .unwrap();
    opened
        .store
        .append(
            4,
            commit(
                "compact",
                5,
                event(
                    "ec",
                    EventData::CompactionRecorded(CompactionRecordedV1 {
                        compaction_id: "compact-1".into(),
                        tree_parent_entry_id: Some("m1".into()),
                        summary: "summary".into(),
                        first_retained_entry_id: "m1".into(),
                        tokens_before: 100,
                        committed_at: 5,
                    }),
                ),
            ),
        )
        .unwrap();
    fill_to_segment_boundary(&opened.store);
    assert_eq!(
        opened
            .store
            .aggregate()
            .accounting
            .summarize_incurred()
            .usage
            .total_tokens,
        14
    );
    drop(opened);

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    let summary = reopened.aggregate.accounting.summarize_incurred();
    assert_eq!(summary.fact_count, 1);
    assert_eq!(summary.usage.total_tokens, 14);
}

#[test]
fn generated_branch_history_converges_live_read_model_and_full_replay() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m0", None, None))))
        .unwrap();
    let mut messages = vec!["m0".to_string()];
    let mut seed = 0x5eed_u64;
    while opened.store.aggregate().revision < 1_000 {
        let revision = opened.store.aggregate().revision + 1;
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let parent = messages[(seed as usize) % messages.len()].clone();
        let data = if revision.is_multiple_of(7) {
            EventData::BranchSelected {
                selected_tree_entry_id: Some(parent.clone()),
                root_base_message_id: Some(parent),
            }
        } else {
            let id = format!("m{revision}");
            messages.push(id.clone());
            message(&id, Some(&parent), Some(&parent))
        };
        opened
            .store
            .append(
                revision - 1,
                commit(
                    &format!("c{revision}"),
                    revision as i64,
                    event(&format!("e{revision}"), data),
                ),
            )
            .unwrap();
    }
    for revision in 1_001..=1_025 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let parent = messages[(seed as usize) % messages.len()].clone();
        let id = format!("m{revision}");
        messages.push(id.clone());
        opened
            .store
            .append(
                revision - 1,
                commit(
                    &format!("c{revision}"),
                    revision as i64,
                    event(
                        &format!("e{revision}"),
                        message(&id, Some(&parent), Some(&parent)),
                    ),
                ),
            )
            .unwrap();
    }
    let live = opened.store.aggregate();
    drop(opened);
    let from_model = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(from_model.aggregate, live);
    drop(from_model);
    std::fs::remove_file(path.join("readmodels/current.json")).unwrap();
    let full = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(full.aggregate, live);
}

#[test]
fn query_catalog_and_current_use_published_models() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    drop(opened);

    let catalog = piko_session_store::query_catalog(&path).unwrap();
    assert_eq!(catalog.facts.message_count, 1);
    let current = piko_session_store::query_current(&path).unwrap();
    assert_eq!(current.revision, 2);
    assert!(current.messages.contains_key("m1"));
}

#[test]
fn query_republishes_stale_models_while_writer_is_attached() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();

    for name in ["catalog.json", "current.json", "trajectory.json"] {
        std::fs::write(path.join("readmodels").join(name), b"broken").unwrap();
    }

    let current = piko_session_store::query_current(&path).unwrap();
    assert_eq!(current.revision, 2);
    for name in ["catalog.json", "current.json", "trajectory.json"] {
        let bytes = std::fs::read(path.join("readmodels").join(name)).unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .unwrap_or_else(|error| panic!("{name} was not republished: {error}"));
    }
    assert_eq!(
        piko_session_store::query_catalog(&path)
            .unwrap()
            .facts
            .revision,
        2
    );
    assert_eq!(
        piko_session_store::query_trajectory(&path)
            .unwrap()
            .revision,
        2
    );
}

#[test]
fn corrupt_current_read_model_is_rebuilt_from_the_journal() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    fill_to_segment_boundary(&opened.store);
    opened
        .store
        .append(
            1_000,
            commit(
                "tail-c1001",
                1_001,
                event("tail-e1001", message("m2", Some("m1"), Some("m1"))),
            ),
        )
        .unwrap();
    drop(opened);

    let replayed = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(replayed.aggregate.revision, 1_001);
    assert_eq!(
        replayed.aggregate.active_root_transcript().unwrap().len(),
        2
    );
    drop(replayed);

    FsOpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path.join("readmodels/current.json"))
        .unwrap()
        .write_all(b"broken")
        .unwrap();
    let rebuilt = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(rebuilt.aggregate.revision, 1_001);
    assert_eq!(rebuilt.aggregate.active_root_transcript().unwrap().len(), 2);
    assert_eq!(
        piko_session_store::inspect_catalog(&path)
            .unwrap()
            .unwrap()
            .facts
            .revision,
        1_001
    );
}

#[test]
fn foreign_generation_current_read_model_is_ignored() {
    let temp = tempdir().unwrap();
    let source_path = temp.path().join("source");
    let target_path = temp.path().join("target");
    let source = SessionStore::create(&source_path, new_session("same-id")).unwrap();
    fill_to_segment_boundary(&source.store);
    let target = SessionStore::create(&target_path, new_session("same-id")).unwrap();
    fill_to_segment_boundary(&target.store);
    target
        .store
        .append(
            1_000,
            commit(
                "target-tail",
                1_001,
                event("target-message", message("m1", None, None)),
            ),
        )
        .unwrap();
    drop(source);
    drop(target);

    std::fs::copy(
        source_path.join("readmodels/current.json"),
        target_path.join("readmodels/current.json"),
    )
    .unwrap();
    let reopened = SessionStore::open(&target_path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.aggregate.revision, 1_001);
}

#[test]
fn incomplete_tail_is_repaired_but_middle_corruption_fails() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    drop(opened);
    let segment = path.join("events/00000000000000000001-open.jsonl");
    FsOpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"{\"partial\":")
        .unwrap();
    let repaired = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert!(repaired.recovery.repaired);
    assert_eq!(repaired.aggregate.revision, 2);
    drop(repaired);

    let mut file = FsOpenOptions::new().append(true).open(&segment).unwrap();
    file.write_all(b"not-json\n").unwrap();
    let error = SessionStore::open(&path, OpenOptions::default()).unwrap_err();
    assert!(matches!(error, StoreError::Corruption { .. }));
}

#[test]
fn every_torn_commit_byte_boundary_recovers_the_verified_prefix() {
    let temp = tempdir().unwrap();
    let base = temp.path().join("base");
    let base_opened = SessionStore::create(&base, new_session("s1")).unwrap();
    drop(base_opened);
    let source = temp.path().join("source");
    copy_session(&base, &source);
    let source_opened = SessionStore::open(&source, OpenOptions::default()).unwrap();
    source_opened
        .store
        .append(
            1,
            commit(
                "c2",
                2,
                RawEvent::optional("e2", "annotation", serde_json::json!({"ok": true})),
            ),
        )
        .unwrap();
    drop(source_opened);
    let base_segment = base.join("events/00000000000000000001-open.jsonl");
    let source_bytes =
        std::fs::read(source.join("events/00000000000000000001-open.jsonl")).unwrap();
    let base_len = std::fs::metadata(&base_segment).unwrap().len() as usize;
    let commit_line = &source_bytes[base_len..];

    for boundary in 0..=commit_line.len() {
        let candidate = temp.path().join(format!("cut-{boundary}"));
        copy_session(&base, &candidate);
        FsOpenOptions::new()
            .append(true)
            .open(candidate.join("events/00000000000000000001-open.jsonl"))
            .unwrap()
            .write_all(&commit_line[..boundary])
            .unwrap();
        let reopened = SessionStore::open(&candidate, OpenOptions::default()).unwrap();
        let expected = if boundary == commit_line.len() { 2 } else { 1 };
        assert_eq!(reopened.aggregate.revision, expected, "boundary {boundary}");
    }
}

fn copy_session(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir(target).unwrap();
    std::fs::create_dir(target.join("events")).unwrap();
    std::fs::copy(source.join("session.json"), target.join("session.json")).unwrap();
    std::fs::copy(
        source.join("events/00000000000000000001-open.jsonl"),
        target.join("events/00000000000000000001-open.jsonl"),
    )
    .unwrap();
}

fn fill_to_segment_boundary(store: &SessionStore) {
    let start = store.aggregate().revision + 1;
    for revision in start..=1_000 {
        store
            .append(
                revision - 1,
                commit(
                    &format!("fill-c{revision}"),
                    revision as i64,
                    RawEvent::optional(
                        format!("fill-e{revision}"),
                        "test_annotation",
                        serde_json::json!({"revision": revision}),
                    ),
                ),
            )
            .unwrap();
    }
}

#[test]
fn unknown_optional_event_replays_and_required_event_requires_upgrade() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(
            1,
            commit(
                "c2",
                2,
                RawEvent::optional("e2", "future_annotation", serde_json::json!({"x": 1})),
            ),
        )
        .unwrap();
    assert_eq!(opened.store.aggregate().revision, 2);

    let mut required = RawEvent::optional("e3", "future_required", serde_json::json!({}));
    required.compatibility.ignorable = false;
    required.compatibility.required_reader_version = piko_session_store::READER_VERSION + 1;
    let error = opened
        .store
        .append(2, commit("c3", 3, required))
        .unwrap_err();
    assert!(matches!(error, StoreError::UpgradeRequired { .. }));
}

#[test]
fn reader_capability_version_is_independent_from_schema_generation() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let mut future_optional =
        RawEvent::optional("optional", "future_optional", serde_json::json!({"x": 1}));
    future_optional.compatibility.required_reader_version = piko_session_store::READER_VERSION + 1;
    opened
        .store
        .append(1, commit("optional", 2, future_optional))
        .unwrap();
    assert_eq!(opened.store.aggregate().revision, 2);
    assert_eq!(piko_session_store::SCHEMA_VERSION, 4);
}

#[test]
fn envelope_extensions_round_trip_and_missing_fields_default() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    let mut raw = event("extension-event", message("m1", None, None));
    raw.extensions.insert(
        "piko.dev/provider-metadata".into(),
        serde_json::json!({"requestId": "r1"}),
    );
    let proposed = ProposedCommit {
        commit_id: "extension-commit".into(),
        committed_at: 2,
        causation_id: None,
        correlation_id: None,
        events: vec![raw],
        extensions: BTreeMap::from([(
            "piko.dev/audit".into(),
            serde_json::json!({"source": "test"}),
        )]),
    };
    let durable = opened.store.append(1, proposed.clone()).unwrap();
    assert_eq!(durable.extensions, proposed.extensions);
    assert_eq!(durable.events[0].extensions, proposed.events[0].extensions);

    let mut value = serde_json::to_value(&durable).unwrap();
    value.as_object_mut().unwrap().remove("extensions");
    value
        .as_object_mut()
        .unwrap()
        .insert("futureEnvelopeField".into(), serde_json::json!(true));
    let decoded: piko_session_store::DurableCommit = serde_json::from_value(value).unwrap();
    assert!(decoded.extensions.is_empty());
    assert_eq!(decoded.events[0].extensions, durable.events[0].extensions);

    drop(opened);
    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    let retry = reopened.store.append(1, proposed).unwrap();
    assert_eq!(retry, durable);
}

#[test]
fn extension_keys_must_be_namespaced() {
    let temp = tempdir().unwrap();
    let opened = SessionStore::create(&temp.path().join("session"), new_session("s1")).unwrap();
    let mut proposed = commit(
        "extension-commit",
        2,
        event("e2", message("m1", None, None)),
    );
    proposed
        .extensions
        .insert("unqualified".into(), serde_json::json!(true));

    let error = opened.store.append(1, proposed).unwrap_err();
    assert!(
        matches!(error, StoreError::InvalidEvent(message) if message.contains("not namespaced"))
    );
    assert_eq!(opened.store.aggregate().revision, 1);
}

#[test]
fn rolls_segment_at_one_thousand_commits() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    for revision in 2..=1_000 {
        opened
            .store
            .append(
                revision - 1,
                commit(
                    &format!("c{revision}"),
                    revision as i64,
                    RawEvent::optional(
                        format!("e{revision}"),
                        "test_annotation",
                        serde_json::json!({"revision": revision}),
                    ),
                ),
            )
            .unwrap();
    }
    let closed = path.join("events/00000000000000000001-00000000000000001000.jsonl");
    let interrupted_open = path.join("events/00000000000000000001-open.jsonl");
    std::fs::rename(&closed, &interrupted_open).unwrap();
    std::fs::remove_file(path.join("events/00000000000000001001-open.jsonl")).unwrap();
    opened
        .store
        .append(
            999,
            commit(
                "c1000",
                1_000,
                RawEvent::optional(
                    "e1000",
                    "test_annotation",
                    serde_json::json!({"revision": 1_000}),
                ),
            ),
        )
        .unwrap();
    assert!(closed.exists());
    assert!(path.join("events/00000000000000001001-open.jsonl").exists());
    assert_eq!(opened.store.verify().unwrap().revision, 1_000);
    assert_eq!(
        piko_session_store::inspect_catalog(&path)
            .unwrap()
            .unwrap()
            .facts
            .revision,
        1_000
    );
}
