use std::collections::BTreeMap;
use std::fs::OpenOptions as FsOpenOptions;
use std::io::Write;

use piko_protocol::{AgentInstanceIdentity, Message, MessageContent, Usage};
use piko_session_store::{
    CompactionRecordedV1, EventData, MessageCommittedV1, NewSession, OpenOptions, ProposedCommit,
    RawEvent, SessionStore, SnapshotStatus, StoreError, UsageAttribution, UsageCorrectedV1,
    UsageQuery, UsageRecordedV1,
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

fn commit(id: &str, at: i64, event: RawEvent) -> ProposedCommit {
    ProposedCommit::one(id, at, event)
}

fn message(id: &str, agent_parent: Option<&str>, tree_parent: Option<&str>) -> EventData {
    EventData::MessageCommitted(MessageCommittedV1 {
        message_id: id.into(),
        agent_instance_id: "root".into(),
        agent_parent_message_id: agent_parent.map(str::to_string),
        tree_parent_entry_id: tree_parent.map(str::to_string),
        execution_id: Some(format!("exec-{id}")),
        source_turn_id: Some(format!("turn-{id}")),
        committed_at: 2,
        message: Message::User {
            content: MessageContent::String(id.into()),
            timestamp: Some(2),
        },
    })
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
fn raw_events_are_cached_on_open_and_append() {
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
    let first = opened.store.raw_events().unwrap();
    assert!(
        first
            .iter()
            .any(|event| event.event.event_type == "trajectory.assembly")
    );
    assert_eq!(opened.store.raw_events().unwrap(), first);
    assert_eq!(opened.store.raw_events_after(2).unwrap().len(), 1);
    drop(opened);

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.store.raw_events().unwrap(), first);
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
            turn_id: Some("t1".into()),
            execution_id: "x1".into(),
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
            execution_id: Some("x1".into()),
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
            turn_id: Some("turn-1".into()),
            execution_id: "exec-1".into(),
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
    opened.store.write_snapshot().unwrap();
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
fn generated_branch_history_converges_live_snapshot_tail_and_full_replay() {
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
    let snapshot = opened.store.write_snapshot().unwrap();
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
    let snapshot_tail = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(snapshot_tail.aggregate, live);
    drop(snapshot_tail);
    std::fs::remove_file(snapshot.path).unwrap();
    let full = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(full.aggregate, live);
}

#[test]
fn snapshot_plus_tail_matches_full_replay_and_corrupt_snapshot_is_disposable() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("s1")).unwrap();
    opened
        .store
        .append(1, commit("c2", 2, event("e2", message("m1", None, None))))
        .unwrap();
    fill_to_segment_boundary(&opened.store);
    let snapshot = opened.store.write_snapshot().unwrap();
    assert_eq!(snapshot.through_revision, 1_000);
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
    assert!(matches!(
        replayed.store.latest_snapshot(),
        SnapshotStatus::Valid(_)
    ));
    drop(replayed);

    FsOpenOptions::new()
        .write(true)
        .truncate(true)
        .open(snapshot.path)
        .unwrap()
        .write_all(b"broken")
        .unwrap();
    let full_replay = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(full_replay.aggregate.revision, 1_001);
    assert_eq!(
        full_replay
            .aggregate
            .active_root_transcript()
            .unwrap()
            .len(),
        2
    );
    for _ in 0..200 {
        if matches!(
            full_replay.store.latest_snapshot(),
            SnapshotStatus::Valid(ref snapshot) if snapshot.through_revision == 1_000
        ) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("invalid boundary snapshot was not rebuilt from the journal prefix");
}

#[test]
fn snapshot_from_another_journal_generation_is_rejected() {
    let temp = tempdir().unwrap();
    let source_path = temp.path().join("source");
    let target_path = temp.path().join("target");
    let source = SessionStore::create(&source_path, new_session("same-id")).unwrap();
    fill_to_segment_boundary(&source.store);
    source.store.write_snapshot().unwrap();
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
    // Boundary snapshots are asynchronous; let both writers finish before
    // replacing the target cache with the foreign-generation snapshot.
    std::thread::sleep(std::time::Duration::from_millis(100));

    std::fs::copy(
        source_path.join("snapshots/00000000000000001000.json"),
        target_path.join("snapshots/00000000000000001000.json"),
    )
    .unwrap();
    let reopened = SessionStore::open(&target_path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.aggregate.revision, 1_001);
    assert!(matches!(
        reopened.store.latest_snapshot(),
        SnapshotStatus::Invalid { .. }
    ));
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
    std::fs::create_dir(target.join("snapshots")).unwrap();
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
    for _ in 0..200 {
        if matches!(
            opened.store.latest_snapshot(),
            SnapshotStatus::Valid(ref snapshot) if snapshot.through_revision == 1_000
        ) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("revision-1000 snapshot was not published at the segment boundary");
}
