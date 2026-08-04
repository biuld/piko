// F-09 / D-26: full clone sanitization and branch-point fork.

use piko_hostd::domain::prompts::{RunKind, WorldStateFacts};

fn linear_messages(
    store: &SessionStore,
    session_id: &str,
    agent_instance_id: &str,
    agent_spec_id: &str,
    ids: &[&str],
) {
    let mut parent = None;
    for (index, id) in ids.iter().enumerate() {
        let committed_at = (index as i64) + 1;
        store
            .commit_message(
                MessageCommit {
                    session_id: session_id.into(),
                    source_turn_id: Some(format!("turn-{id}")),
                    execution_id: format!("exec-{id}"),
                    agent_instance_id: agent_instance_id.into(),
                    message_id: (*id).into(),
                    parent_message_id: parent.clone(),
                    message: Message::User {
                        content: MessageContent::String(format!("body-{id}")),
                        timestamp: Some(committed_at),
                    },
                    committed_at,
                },
                agent_spec_id,
            )
            .expect("commit linear message");
        parent = Some((*id).to_string());
    }
}

#[test]
fn branch_point_fork_keeps_ancestor_path_only() {
    let temp = tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let created = repo.create("/project").unwrap();
    let session_id = created.state.session_id.clone();
    let session_dir = created.path.clone();
    let root = created
        .state
        .active_agent_instance_id
        .clone()
        .expect("root agent");

    let store = SessionStore::new(&session_dir);
    linear_messages(&store, &session_id, &root, "main", &["m1", "m2", "m3"]);

    let forked = repo
        .fork(&session_id, &session_dir, Some("m2"))
        .expect("branch-point fork");

    assert_ne!(forked.state.session_id, session_id);
    assert_eq!(forked.state.current_leaf_id.as_deref(), Some("m2"));
    let forked_ids: Vec<&str> = forked
        .state
        .entries
        .iter()
        .filter_map(|entry| match entry {
            piko_hostd::api::SessionTreeEntry::Message(m) => Some(m.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(forked_ids, vec!["m1", "m2"]);
    assert!(forked.state.world_state_baseline.is_none());

    let after_source = repo.load_by_path(&session_dir).expect("reload source");
    let source_message_ids: Vec<&str> = after_source
        .state
        .entries
        .iter()
        .filter_map(|entry| match entry {
            piko_hostd::api::SessionTreeEntry::Message(m) => Some(m.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(source_message_ids, vec!["m1", "m2", "m3"]);
}

#[test]
fn branch_point_fork_rejects_unknown_entry() {
    let temp = tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let created = repo.create("/project").unwrap();
    let session_id = created.state.session_id.clone();
    let session_dir = created.path.clone();
    let root = created
        .state
        .active_agent_instance_id
        .clone()
        .expect("root agent");

    linear_messages(
        &SessionStore::new(&session_dir),
        &session_id,
        &root,
        "main",
        &["m1"],
    );

    let err = repo
        .fork(&session_id, &session_dir, Some("missing-entry"))
        .expect_err("unknown entry must fail");
    let message = err.to_string();
    assert!(
        message.contains("unknown tree entry"),
        "unexpected error: {message}"
    );
}

#[test]
fn full_clone_clears_world_state_baseline_and_transient_queues() {
    let temp = tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let created = repo.create("/project").unwrap();
    let session_id = created.state.session_id.clone();
    let session_dir = created.path.clone();
    let root = created
        .state
        .active_agent_instance_id
        .clone()
        .expect("root agent");

    let store = SessionStore::new(&session_dir);
    linear_messages(&store, &session_id, &root, "main", &["m1", "m2"]);
    store
        .update_manifest(|manifest| {
            manifest.world_state_baseline = Some(WorldStateFacts {
                session_id: Some(session_id.clone()),
                agent_instance_id: Some(root.clone()),
                operation_id: Some("op-1".into()),
                run_kind: RunKind::Continuation,
                model: Some("gpt-test".into()),
            });
            manifest.agent_inbox.push(piko_protocol::AgentInboxItem {
                report_id: "r1".into(),
                recipient_agent_instance_id: root.clone(),
                source_agent_instance_id: "child".into(),
                report: AgentRunReport {
                    agent_instance_id: "child".into(),
                    report_id: "r1".into(),
                    outcome: piko_protocol::ExecutionOutcome::Succeeded {
                        usage: Default::default(),
                    },
                    summary: "done".into(),
                    usage: Default::default(),
                    artifacts: Vec::new(),
                },
                committed_at: 1,
                consumed_at: None,
            });
        })
        .unwrap();

    let forked = repo.fork(&session_id, &session_dir, None).expect("full clone");
    let forked_store = SessionStore::new(&forked.path);
    let forked_manifest = forked_store.load_manifest().unwrap();
    assert!(forked_manifest.world_state_baseline.is_none());
    assert!(forked_manifest.agent_inbox.is_empty());
    assert!(forked_manifest.agent_executions.is_empty());
    assert!(forked_manifest.agent_input_queue.is_empty());

    let forked_messages: Vec<&str> = forked
        .state
        .entries
        .iter()
        .filter_map(|entry| match entry {
            piko_hostd::api::SessionTreeEntry::Message(m) => Some(m.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(forked_messages, vec!["m1", "m2"]);
}
