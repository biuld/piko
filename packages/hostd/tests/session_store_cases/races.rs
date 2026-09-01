#[tokio::test]
async fn interrupt_during_pending_action_keeps_requires_action_until_resolve() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let root_input_id = "input-r9";
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                request_id: root_input_id.into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-r9".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    root_input_id,
                    None,
                    "control input",
                    1,
                ),
                input_message_id: "message-r9".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 1,
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::PendingActionRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                action: piko_protocol::PendingActionSummary {
                    action_id: "approval-r9".into(),
                    kind: "approval".into(),
                    summary: Some("shell".into()),
                },
                requested_at: 2,
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::InterruptRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                requested_at: 3,
            },
        )
        .await
        .unwrap();

    let snapshot = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        snapshot.foreground,
        piko_protocol::AgentForeground::RequiresAction
    );
    assert_eq!(
        snapshot.active_work.unwrap().state,
        piko_protocol::AgentWorkViewState::Cancelling
    );
    assert_eq!(snapshot.pending_action.unwrap().action_id, "approval-r9");

    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::PendingActionResolved {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                action_id: "approval-r9".into(),
                resolved_at: 4,
            },
        )
        .await
        .unwrap();
    let snapshot = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        snapshot.foreground,
        piko_protocol::AgentForeground::Cancelling
    );
    assert!(snapshot.pending_action.is_none());
}
