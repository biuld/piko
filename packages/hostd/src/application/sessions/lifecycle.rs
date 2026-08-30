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
        let recovery_events = if live_turn_run {
            Vec::new()
        } else if let Some(path) = session_path {
            let store = session_store_factory.open(path);
            let projection = store.load_projection().await.map_err(storage_error)?;
            let mut incomplete = Vec::new();
            for execution in projection.agent_executions.values() {
                if !matches!(
                    execution.status,
                    piko_protocol::ExecutionStatus::Accepted
                        | piko_protocol::ExecutionStatus::Running
                ) {
                    continue;
                }
                let root_input_id = execution
                    .source_turn_id
                    .clone()
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| execution.request_id.clone());
                if root_input_id.is_empty() {
                    continue;
                }
                incomplete.push(root_input_id);
            }
            let mut reports = Vec::with_capacity(incomplete.len());
            for root_input_id in incomplete {
                reports.push((
                    root_input_id.clone(),
                    store
                        .agent_report_for_turn(&root_input_id)
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
            Vec::new()
        } else {
            Vec::new()
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
            recovery_events,
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
    use piko_protocol::AgentDurableCommand;

    use super::*;

    #[tokio::test]
    async fn session_open_interrupts_incomplete_agent_work() {
        let mut state = crate::domain::sessions::HostState::new();
        let crate::api::CommandResult::SessionCreated { session_id, .. } =
            state.create_session("/project")
        else {
            unreachable!()
        };
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
                AgentDurableCommand::AgentInputProcessingStarted {
                    agent_instance_id: root.agent_instance_id.clone(),
                    root_input_id: "request-recovered".into(),
                    request_id: "request-recovered".into(),
                    source_turn_id: Some("request-recovered".into()),
                    detached_recipient_agent_instance_id: None,
                    prompt_assembly_version: 1,
                    prompt_digest: "prompt-recovered".into(),
                    started_at: 2,
                    input: piko_protocol::AgentInput {
                        input_id: "request-recovered".into(),
                        request_id: "request-recovered".into(),
                        session_id: session_id.clone(),
                        agent_instance_id: root.agent_instance_id.clone(),
                        origin: piko_protocol::AgentInputOrigin::User,
                        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
                        content: piko_protocol::MessageContent::String("recover me".into()),
                        submitted_at: 2,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
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

        assert!(
            events
                .iter()
                .any(|event| matches!(event, ServerMessage::SessionReconciled(_)))
        );
        let report = store
            .agent_report_for_turn("request-recovered")
            .unwrap()
            .expect("recovery report");
        assert!(matches!(
            report.outcome,
            piko_protocol::ExecutionOutcome::Cancelled { .. }
        ));

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
                .any(|event| matches!(event, ServerMessage::SessionReconciled(_)))
        );
        assert_eq!(
            store
                .agent_report_for_turn("request-recovered")
                .unwrap()
                .expect("stable recovery report")
                .report_id,
            report.report_id
        );
    }
}

#[cfg(test)]
#[path = "lifecycle_live_tests.rs"]
mod live_tests;
