use std::path::Path;

use crate::application::host_app::HostApp;
use crate::domain::sessions::transcript_messages_from_session_entries;
use crate::ports::ResumeAgent;

impl HostApp {
    /// Reconstruct the root AgentInstance's resume state (transcript +
    /// head/committed message ids) from either the in-memory session tree or,
    /// failing that, the durable journal aggregate.
    pub(super) async fn resume_root_agent_for_session(
        &self,
        session_id: &str,
        session_dir: &Path,
        root_agent_instance_id: &str,
    ) -> Option<ResumeAgent> {
        let session = {
            let state = self.state.lock().await;
            state.session(session_id).ok().cloned()
        };
        match session {
            Some(session) => {
                let session_transcript = transcript_messages_from_session_entries(
                    &session.entries,
                    session.current_leaf_id.as_deref(),
                );
                if session.current_leaf_id.is_none() || !session_transcript.is_empty() {
                    let active_branch = crate::domain::compaction::active_branch_entries(
                        &session.entries,
                        session.current_leaf_id.as_deref(),
                    );
                    let transcript_seq = session
                        .entries
                        .iter()
                        .filter_map(|entry| match entry {
                            piko_protocol::SessionTreeEntry::Message(message)
                                if message.agent_instance_id == root_agent_instance_id =>
                            {
                                Some(message.transcript_seq)
                            }
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0);
                    let head_message_id =
                        active_branch.iter().rev().find_map(|entry| match entry {
                            piko_protocol::SessionTreeEntry::Message(message)
                                if message.agent_instance_id == root_agent_instance_id =>
                            {
                                Some(message.id.clone())
                            }
                            _ => None,
                        });
                    Some(ResumeAgent {
                        agent_instance_id: root_agent_instance_id.to_string(),
                        state: piko_protocol::agent_runtime::AgentResumeState {
                            head_message_id,
                            transcript_seq,
                            committed_message_ids: session
                                .entries
                                .iter()
                                .filter_map(|entry| match entry {
                                    piko_protocol::SessionTreeEntry::Message(message) => {
                                        Some(message.id.clone())
                                    }
                                    _ => None,
                                })
                                .collect(),
                            transcript: session_transcript,
                        },
                    })
                } else {
                    self.session_store_factory
                        .open(session_dir)
                        .load_agent(session_id, root_agent_instance_id)
                        .await
                        .ok()
                        .filter(|recovered| !recovered.transcript.is_empty())
                        .map(|recovered| ResumeAgent {
                            agent_instance_id: root_agent_instance_id.to_string(),
                            state: piko_protocol::agent_runtime::AgentResumeState {
                                transcript: recovered
                                    .transcript
                                    .iter()
                                    .map(|message| message.message.clone())
                                    .collect(),
                                head_message_id: recovered.head_message_id.clone(),
                                transcript_seq: recovered.last_transcript_seq,
                                committed_message_ids: recovered
                                    .transcript
                                    .iter()
                                    .map(|message| message.id.clone())
                                    .collect(),
                            },
                        })
                }
            }
            None => None,
        }
    }
}
