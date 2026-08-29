use std::path::PathBuf;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::ports::session_store::SessionStoreFactory;
use crate::ports::storage_types::SessionStorageError;
use crate::util::{now_ms, storage_error};

use super::helpers::{server_response_ok, session_opened_messages, session_reconciled_message};

impl HostApp {
    pub(super) async fn session_open_response(
        state: &mut crate::domain::sessions::HostState,
        command_id: &str,
        session_id: String,
        session_path: Option<&std::path::Path>,
        session_store_factory: &dyn SessionStoreFactory,
        live_turn_run: bool,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        if let Some(path) = session_path {
            let store = session_store_factory.open(path);
            let projection = store.load_projection().await.map_err(storage_error)?;
            for queued in projection.agent_input_queue {
                let Some(turn_id) = queued.request.source_turn_id.as_deref() else {
                    continue;
                };
                let message = match &queued.request.content {
                    piko_protocol::MessageContent::String(text) => text.as_str(),
                    piko_protocol::MessageContent::Blocks(_) => "",
                };
                state.restore_turn(
                    &session_id,
                    turn_id,
                    &queued.request.agent_instance_id,
                    message,
                    crate::api::TurnStatus::Queued,
                )?;
            }
            for execution in projection.agent_executions.into_values() {
                if !matches!(
                    execution.status,
                    piko_protocol::ExecutionStatus::Accepted
                        | piko_protocol::ExecutionStatus::Running
                ) {
                    continue;
                }
                let Some(turn_id) = execution.source_turn_id.as_deref() else {
                    continue;
                };
                state.restore_turn(
                    &session_id,
                    turn_id,
                    &execution.agent_instance_id,
                    "",
                    crate::api::TurnStatus::Running,
                )?;
            }
        }
        let active_turn_ids = state
            .session(&session_id)?
            .active_turns
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let interrupt_events = if live_turn_run {
            Vec::new()
        } else if let Some(path) = session_path {
            let store = session_store_factory.open(path);
            let mut reports = Vec::with_capacity(active_turn_ids.len());
            for turn_id in &active_turn_ids {
                reports.push((
                    turn_id.clone(),
                    store
                        .agent_report_for_turn(turn_id)
                        .await
                        .map_err(storage_error)?,
                ));
            }
            if reports.iter().any(|(_, report)| report.is_none()) {
                store
                    .interrupt_incomplete_agent_executions()
                    .await
                    .map_err(storage_error)?;
            }
            let mut events = Vec::with_capacity(reports.len());
            for (turn_id, report) in reports {
                let report = match report {
                    Some(report) => Some(report),
                    None => store
                        .agent_report_for_turn(&turn_id)
                        .await
                        .map_err(storage_error)?,
                };
                let event = match report.map(|report| report.outcome) {
                    Some(piko_protocol::ExecutionOutcome::Succeeded { .. }) => {
                        state.complete_turn(&session_id, &turn_id)?
                    }
                    Some(piko_protocol::ExecutionOutcome::Cancelled { .. }) => {
                        state.cancel_turn(&session_id, &turn_id)?
                    }
                    Some(piko_protocol::ExecutionOutcome::Failed { error }) => {
                        state.fail_turn(&session_id, &turn_id, error)?
                    }
                    None => state.fail_turn(
                        &session_id,
                        &turn_id,
                        "turn interrupted: session reopened without a live execution",
                    )?,
                };
                events.extend(state.with_usage_projection(event, None));
            }
            events
        } else {
            state.finalize_interrupted_turns(&session_id)?
        };
        if let Some(path) = session_path {
            let store = session_store_factory.open(path);
            crate::application::turns::projection::reconcile_committed_messages(
                state,
                store.as_ref(),
                &session_id,
            )
            .await?;
        }
        let snapshot = state.snapshot(&session_id)?;
        let agents = state.get_agent_list(&session_id);
        Ok(session_opened_messages(
            command_id,
            session_id,
            snapshot,
            agents,
            interrupt_events,
        ))
    }

    pub(crate) async fn apply_session_create(
        &self,
        command_id: &str,
        cwd: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        if let Some(storage) = &self.storage {
            let persisted = storage.create(&cwd).await.map_err(storage_error)?;
            let session_id = persisted.state.session_id.clone();
            let session_path = persisted.path.clone();
            self.session_paths
                .lock()
                .await
                .insert(session_id.clone(), session_path.clone());
            self.state.lock().await.insert_session(persisted.state);
            let (snapshot, agents) = self.session_view(&session_id).await?;
            Ok(vec![
                server_response_ok(
                    command_id,
                    crate::api::CommandResult::SessionCreated {
                        session_id: session_id.clone(),
                        cwd,
                        timestamp: now_ms(),
                    },
                ),
                session_reconciled_message(
                    session_id,
                    piko_protocol::ReconcileReason::InitialHydration,
                    snapshot,
                    agents,
                ),
            ])
        } else {
            let mut state = self.state.lock().await;
            let created = state.create_session(cwd.clone());
            let session_id = match &created {
                crate::api::CommandResult::SessionCreated { session_id, .. } => session_id.clone(),
                other => {
                    return Err(ProtocolError::InvalidCommand(format!(
                        "unexpected create_session result: {other:?}"
                    )));
                }
            };
            drop(state);
            // In-memory host configurations still use an ephemeral journal
            // for Turn execution. Create it with the session itself so the
            // first chat does not pay genesis durability latency before its
            // acceptance response.
            self.ensure_turn_session_dir(&session_id, &cwd).await?;
            let (snapshot, agents) = self.session_view(&session_id).await?;
            Ok(vec![
                server_response_ok(command_id, created),
                session_reconciled_message(
                    session_id,
                    piko_protocol::ReconcileReason::InitialHydration,
                    snapshot,
                    agents,
                ),
            ])
        }
    }

    pub(crate) async fn apply_session_open(
        &self,
        command_id: &str,
        session_id: String,
        session_path: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let live_turn_run = self
            .turn_runner
            .lock()
            .await
            .clone()
            .has_active_session_run(&session_id)
            .await;
        let known_session_path = self.session_paths.lock().await.get(&session_id).cloned();
        let mut state = self.state.lock().await;

        // A same-process reopen must preserve the live Turn and its in-memory
        // projection instead of reloading/interruption-recovering durable state.
        if live_turn_run && state.has_session(&session_id) {
            let messages = Self::session_open_response(
                &mut state,
                command_id,
                session_id.clone(),
                known_session_path.as_deref(),
                self.session_store_factory.as_ref(),
                true,
            )
            .await?;
            drop(state);
            return Ok(self.enrich_reconcile_messages(&session_id, messages).await);
        }

        // 1. If session_path is provided, load that session directory.
        if let (Some(path_str), Some(storage)) = (session_path, &self.storage) {
            let path = PathBuf::from(path_str);
            let persisted = storage.load_by_path(&path).await.map_err(|err| match err {
                SessionStorageError::NotFound(_) => {
                    ProtocolError::SessionNotFound(session_id.clone())
                }
                _ => ProtocolError::InvalidCommand(format!("invalid session: {}", err)),
            })?;
            let opened_id = persisted.state.session_id.clone();
            if opened_id != session_id {
                return Err(ProtocolError::InvalidCommand(format!(
                    "session path id mismatch: requested {}, found {}",
                    session_id, opened_id
                )));
            }
            self.session_paths
                .lock()
                .await
                .insert(opened_id.clone(), persisted.path.clone());
            state.insert_session(persisted.state);
            let path = persisted.path.clone();
            let messages = Self::session_open_response(
                &mut state,
                command_id,
                opened_id.clone(),
                Some(&path),
                self.session_store_factory.as_ref(),
                false,
            )
            .await?;
            drop(state);
            return Ok(self.enrich_reconcile_messages(&opened_id, messages).await);
        }

        // 2. Otherwise, check if it's already in memory.
        if state.has_session(&session_id) {
            let messages = Self::session_open_response(
                &mut state,
                command_id,
                session_id.clone(),
                known_session_path.as_deref(),
                self.session_store_factory.as_ref(),
                false,
            )
            .await?;
            drop(state);
            return Ok(self.enrich_reconcile_messages(&session_id, messages).await);
        }

        // 3. Resolve the session directory from identity files, then load
        //    that one current-state read model.
        if let Some(storage) = &self.storage {
            let resolved = storage
                .resolve_session_dir(None, &session_id)
                .await
                .map_err(storage_error)?;
            if let Some(path) = resolved {
                let persisted = storage.load_by_path(&path).await.map_err(|err| match err {
                    SessionStorageError::NotFound(_) => {
                        ProtocolError::SessionNotFound(session_id.clone())
                    }
                    _ => ProtocolError::InvalidCommand(format!("invalid session: {}", err)),
                })?;
                let opened_id = persisted.state.session_id.clone();
                self.session_paths
                    .lock()
                    .await
                    .insert(opened_id.clone(), persisted.path.clone());
                state.insert_session(persisted.state);
                let path = persisted.path.clone();
                let messages = Self::session_open_response(
                    &mut state,
                    command_id,
                    opened_id.clone(),
                    Some(&path),
                    self.session_store_factory.as_ref(),
                    false,
                )
                .await?;
                drop(state);
                return Ok(self.enrich_reconcile_messages(&opened_id, messages).await);
            }
        }

        Err(ProtocolError::SessionNotFound(session_id))
    }

    pub(crate) async fn apply_session_list(
        &self,
        command_id: &str,
        scope: crate::api::SessionListScope,
        cwd: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let list_cwd = match scope {
            crate::api::SessionListScope::CurrentFolder => {
                let resolved_cwd = cwd
                    .or_else(|| {
                        std::env::current_dir()
                            .ok()
                            .and_then(|path| path.to_str().map(String::from))
                    })
                    .unwrap_or_else(|| ".".to_string());
                Some(resolved_cwd)
            }
            crate::api::SessionListScope::All => None,
        };

        let sessions = if let Some(storage) = &self.storage {
            storage
                .summaries(list_cwd.as_deref())
                .await
                .map_err(storage_error)?
        } else {
            let state = self.state.lock().await;
            let mut list = state.list_sessions();
            if let Some(ref filter_cwd) = list_cwd {
                list.retain(|s| s.cwd == *filter_cwd);
            }
            list
        };

        Ok(vec![server_response_ok(
            command_id,
            crate::api::CommandResult::SessionListed {
                sessions,
                timestamp: now_ms(),
            },
        )])
    }
}

#[cfg(test)]
mod tests {
    use piko_orchd_api::AgentCommitPort;
    use piko_protocol::{AgentDurableCommand, AgentRunReport, ExecutionOutcome};

    use super::*;

    #[tokio::test]
    async fn session_open_recovers_turn_terminal_from_durable_root_report() {
        let mut state = crate::domain::sessions::HostState::new();
        let crate::api::CommandResult::SessionCreated { session_id, .. } =
            state.create_session("/project")
        else {
            unreachable!()
        };
        let root_agent_instance_id = format!("agent_{session_id}_root");
        let (turn_id, _) = state
            .start_turn(&session_id, &root_agent_instance_id, "recover me")
            .unwrap();
        state
            .apply_turn_input_disposition(
                &session_id,
                &turn_id,
                piko_protocol::InputDisposition::Accepted,
            )
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let store = crate::infra::storage::SessionStore::create_session(
            temp.path(),
            session_id.clone(),
            "/project".into(),
            1,
        )
        .unwrap();
        let root = store.ensure_root_agent("main").unwrap();
        store
            .commit_agent_command(
                &session_id,
                AgentDurableCommand::RunStarted {
                    agent_instance_id: root.agent_instance_id.clone(),
                    run_id: "run-recovered".into(),
                    internal_execution_id: "exec-recovered".into(),
                    request_id: "request-recovered".into(),
                    source_turn_id: Some(turn_id.clone()),
                    detached_recipient_agent_instance_id: None,
                    prompt_assembly_version: 1,
                    prompt_digest: "prompt-recovered".into(),
                    started_at: 2,
                    input: None,
                },
            )
            .await
            .unwrap();
        store
            .commit_agent_command(
                &session_id,
                AgentDurableCommand::RunTerminal {
                    run_id: "run-recovered".into(),
                    report: AgentRunReport {
                        agent_instance_id: root.agent_instance_id,
                        report_id: "report-recovered".into(),
                        outcome: ExecutionOutcome::Succeeded {
                            usage: Default::default(),
                        },
                        summary: "done".into(),
                        usage: Default::default(),
                        artifacts: Vec::new(),
                    },
                    finished_at: 3,
                },
            )
            .await
            .unwrap();

        let factory = crate::adapters::storage::FsSessionStoreFactory;
        let events = HostApp::session_open_response(
            &mut state,
            "open-1",
            session_id.clone(),
            Some(temp.path()),
            &factory,
            false,
        )
        .await
        .unwrap();

        assert!(events.iter().any(|event| matches!(
            event,
            ServerMessage::TurnLifecycle(crate::api::TurnEvent::Completed {
                turn_id: completed,
                ..
            }) if completed == &turn_id
        )));
        assert!(state.session(&session_id).unwrap().active_turns.is_empty());

        let replay = HostApp::session_open_response(
            &mut state,
            "open-2",
            session_id,
            Some(temp.path()),
            &factory,
            false,
        )
        .await
        .unwrap();
        assert!(
            replay
                .iter()
                .all(|event| !matches!(event, ServerMessage::TurnLifecycle(_)))
        );
    }
}

#[cfg(test)]
#[path = "lifecycle_live_tests.rs"]
mod live_tests;
