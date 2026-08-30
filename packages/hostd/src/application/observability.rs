use crate::api::{ProtocolError, RolloutPage, TranscriptCommittedEvent};
use crate::application::host_app::HostApp;
use crate::domain::sessions::{file_change_from_message, merge_file_change, render_turn_diff};

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;
const CURSOR_PREFIX: &str = "seq:";

impl HostApp {
    /// Read one bounded page from the durable AgentInstance rollout.
    ///
    /// This path never creates a session store: a session without a known
    /// durable directory is reported as unavailable.
    pub(crate) async fn rollout_page(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        after_cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<RolloutPage, ProtocolError> {
        let after_seq = parse_rollout_cursor(after_cursor)?;
        let limit = limit
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT);
        let session_dir = self
            .session_paths
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!(
                    "rollout unavailable for session {session_id}"
                ))
            })?;
        let recovered = self
            .session_store_factory
            .open(&session_dir)
            .load_agent(session_id, agent_instance_id)
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        let mut matching = recovered
            .transcript
            .into_iter()
            .filter(|message| message.transcript_seq > after_seq);
        let mut page_records = matching.by_ref().take(limit + 1).collect::<Vec<_>>();
        let has_more = page_records.len() > limit;
        if has_more {
            page_records.pop();
        }
        let next_cursor = has_more
            .then(|| {
                page_records
                    .last()
                    .map(|item| rollout_cursor(item.transcript_seq))
            })
            .flatten();
        let items = page_records
            .into_iter()
            .map(|message| TranscriptCommittedEvent {
                session_id: session_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                agent_id: message.agent_spec_id,
                source_turn_id: message.source_turn_id.unwrap_or_default(),
                message_id: message.id,
                transcript_seq: message.transcript_seq,
                message: message.message,
            })
            .collect();
        Ok(RolloutPage {
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            items,
            next_cursor,
        })
    }

    /// Read a turn diff from live state or rebuild it from durable rollouts.
    pub(crate) async fn turn_diff(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::TurnDiffEvent>, ProtocolError> {
        if let Some(diff) = self.state.lock().await.turn_diff(session_id, turn_id) {
            return Ok(Some(diff));
        }
        let session_dir = self
            .session_paths
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!(
                    "turn diff unavailable for session {session_id}"
                ))
            })?;
        let store = self.session_store_factory.open(&session_dir);
        let agents = store
            .agent_instances()
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        let mut changes = Vec::new();
        for agent in agents {
            let recovered = store
                .load_agent(session_id, &agent.identity.agent_instance_id)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            for committed in recovered.transcript {
                if committed.source_turn_id.as_deref() != Some(turn_id) {
                    continue;
                }
                if let Some(change) = file_change_from_message(&committed.message) {
                    merge_file_change(&mut changes, change);
                }
            }
        }
        Ok((!changes.is_empty()).then(|| piko_protocol::TurnDiffEvent {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            unified_diff: render_turn_diff(&changes),
            files: changes,
        }))
    }
}

fn parse_rollout_cursor(cursor: Option<&str>) -> Result<u64, ProtocolError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    cursor
        .strip_prefix(CURSOR_PREFIX)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| ProtocolError::InvalidCommand("invalid rollout cursor".into()))
}

fn rollout_cursor(transcript_seq: u64) -> String {
    format!("{CURSOR_PREFIX}{transcript_seq}")
}

#[cfg(test)]
mod tests {
    use piko_protocol::execution::MessageCommit;

    use super::{HostApp, parse_rollout_cursor, rollout_cursor};
    use crate::api::Message;
    use crate::infra::storage::SessionStore;

    #[test]
    fn rollout_cursor_is_opaque_and_round_trips() {
        let cursor = rollout_cursor(42);
        assert_eq!(parse_rollout_cursor(Some(&cursor)).unwrap(), 42);
        assert!(parse_rollout_cursor(Some("42")).is_err());
        assert!(parse_rollout_cursor(Some("seq:nope")).is_err());
    }

    #[tokio::test]
    async fn turn_diff_query_reads_live_host_state() {
        let app = HostApp::new();
        let session_id = {
            let mut state = app.state.lock().await;
            let crate::api::CommandResult::SessionCreated { session_id, .. } =
                state.create_session("/workspace")
            else {
                unreachable!()
            };
            state.session_mut(&session_id).unwrap().entries.push(
                crate::api::SessionTreeEntry::Message(crate::api::MessageEntry {
                    id: "result-1".into(),
                    parent_id: None,
                    timestamp: "1".into(),
                    agent_id: "main".into(),
                    agent_instance_id: "agent-1".into(),
                    source_turn_id: "input-1".into(),
                    transcript_seq: 1,
                    message: Message::ToolResult {
                        tool_call_id: "call-1".into(),
                        tool_name: Some("write".into()),
                        content: Vec::new(),
                        details: Some(serde_json::json!({
                            "_pikoFileChange": {
                                "path": "a.txt",
                                "before": "old\n",
                                "after": "new\n"
                            }
                        })),
                        is_error: Some(false),
                        timestamp: Some(1),
                    },
                }),
            );
            (session_id, "input-1".to_string())
        };
        let diff = app
            .turn_diff(&session_id.0, &session_id.1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(diff.files.len(), 1);
        assert!(diff.unified_diff.contains("-old"));
        assert!(diff.unified_diff.contains("+new"));
    }

    #[tokio::test]
    async fn turn_diff_query_rebuilds_from_durable_rollout() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::create_session(
            temp.path(),
            "session-durable".into(),
            "/workspace".into(),
            1,
        )
        .unwrap();
        let root = store.ensure_root_agent("main").unwrap();
        store
            .commit_message(
                MessageCommit {
                    session_id: "session-durable".into(),
                    source_turn_id: Some("turn-durable".into()),
                    root_input_id: "input-1".into(),
                    agent_instance_id: root.agent_instance_id,
                    message_id: "result-1".into(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::ToolResult {
                        tool_call_id: "call-1".into(),
                        tool_name: Some("write".into()),
                        content: Vec::new(),
                        details: Some(serde_json::json!({
                            "_pikoFileChange": {
                                "path": "a.txt",
                                "before": "old",
                                "after": "new"
                            }
                        })),
                        is_error: Some(false),
                        timestamp: Some(2),
                    },
                    committed_at: 2,
                },
                "main",
            )
            .unwrap();

        let app = HostApp::new();
        app.session_paths
            .lock()
            .await
            .insert("session-durable".into(), temp.path().to_path_buf());
        let diff = app
            .turn_diff("session-durable", "turn-durable")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(diff.files[0].before.as_deref(), Some("old"));
        assert_eq!(diff.files[0].after.as_deref(), Some("new"));
    }
}
