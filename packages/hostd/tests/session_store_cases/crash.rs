#[tokio::test]
async fn applied_steer_is_not_redelivered_after_interrupt_recovery() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let root_input_id = "input-c5";
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                request_id: root_input_id.into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-c5".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    root_input_id,
                    None,
                    "c5 input",
                    1,
                ),
                input_message_id: "message-c5".into(),
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
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput {
                        input_id: "steer-c5".into(),
                        request_id: "steer-c5".into(),
                        session_id: "session-1".into(),
                        agent_instance_id: root.agent_instance_id.clone(),
                        origin: piko_protocol::AgentInputOrigin::User,
                        delivery: piko_protocol::AgentInputDelivery::SteerActive,
                        content: MessageContent::String("steer".into()),
                        submitted_at: 2,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: piko_protocol::AgentInputDisposition::PendingSteer,
                    root_input_id: Some(root_input_id.into()),
                    admitted_at: 2,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_steer(
            piko_protocol::agent_work::MessageCommit {
                session_id: "session-1".into(),
                root_input_id: root_input_id.into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "message-steer-c5".into(),
                parent_message_id: Some("message-c5".into()),
                tree_parent_entry_id: None,
                message: piko_protocol::Message::User {
                    content: MessageContent::String("steer".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
            piko_protocol::AgentInputDispositionChange {
                agent_instance_id: root.agent_instance_id.clone(),
                input_id: "steer-c5".into(),
                disposition: piko_protocol::AgentInputDisposition::AppliedToStep,
                root_input_id: Some(root_input_id.into()),
                model_step_id: Some("input-c5:step_1".into()),
                changed_at: 2,
            },
        )
        .unwrap();

    assert_eq!(store.interrupt_incomplete_agent_work().unwrap(), 1);
    let reopened = SessionStore::new(temp.path());
    let work = reopened
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert!(work.pending_steers.is_empty());
    assert!(work.active_work.is_none());
}

#[tokio::test]
async fn interrupt_requested_then_recovery_still_finishes_the_root() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let root_input_id = "input-c6";
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                request_id: root_input_id.into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-c6".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    root_input_id,
                    None,
                    "c6 input",
                    1,
                ),
                input_message_id: "message-c6".into(),
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
            AgentDurableCommand::InterruptRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                requested_at: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(store.interrupt_incomplete_agent_work().unwrap(), 1);
    let snapshot = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert!(snapshot.active_work.is_none());
    let projection = store.load_projection().unwrap();
    let execution = projection.root_inputs.get(root_input_id).unwrap();
    assert_eq!(
        execution.status,
        piko_protocol::AgentWorkProcessingStatus::Cancelled
    );
}

#[tokio::test]
async fn duplicate_cancel_of_already_cancelled_input_is_idempotent() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput {
                        input_id: "follow-c10".into(),
                        request_id: "follow-c10".into(),
                        session_id: "session-1".into(),
                        agent_instance_id: root.agent_instance_id.clone(),
                        origin: piko_protocol::AgentInputOrigin::User,
                        delivery: piko_protocol::AgentInputDelivery::FollowUp,
                        content: MessageContent::String("queued".into()),
                        submitted_at: 1,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 1,
                },
            },
        )
        .await
        .unwrap();
    let cancel = AgentDurableCommand::AgentInputDispositionChanged {
        change: piko_protocol::AgentInputDispositionChange {
            agent_instance_id: root.agent_instance_id.clone(),
            input_id: "follow-c10".into(),
            disposition: piko_protocol::AgentInputDisposition::Cancelled,
            root_input_id: None,
            model_step_id: None,
            changed_at: 2,
        },
    };
    let first = store
        .commit_agent_command("session-1", cancel.clone())
        .await
        .unwrap();
    let second = store
        .commit_agent_command("session-1", cancel)
        .await
        .unwrap();
    assert_eq!(first.revision, second.revision);
    assert!(store
        .agent_queued_inputs(&root.agent_instance_id)
        .unwrap()
        .is_empty());
}
