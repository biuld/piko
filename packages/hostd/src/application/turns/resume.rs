use std::path::Path;

use crate::application::host_app::HostApp;
use crate::domain::sessions::transcript_messages_from_session_entries;
use crate::ports::ResumeRootAgent;

impl HostApp {
    /// Reconstruct the root AgentInstance's resume state (transcript +
    /// head/committed message ids) from either the in-memory session tree or,
    /// failing that, the durable AgentInstance shard on disk.
    pub(super) async fn resume_root_agent_for_session(
        &self,
        session_id: &str,
        session_dir: &Path,
        root_agent_instance_id: &str,
    ) -> Option<ResumeRootAgent> {
        let state = self.state.lock().await;
        match state.session(session_id) {
            Ok(session) => {
                let session_transcript = transcript_messages_from_session_entries(&session.entries);
                if !session_transcript.is_empty() {
                    let transcript_seq = self
                        .session_store_factory
                        .open(session_dir)
                        .load_agent(session_id, root_agent_instance_id)
                        .ok()
                        .map(|recovered| recovered.last_transcript_seq)
                        .unwrap_or_else(|| {
                            session
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
                                .unwrap_or(0)
                        });
                    let head_message_id = session
                        .task_heads
                        .get(root_agent_instance_id)
                        .cloned()
                        .or_else(|| session.current_leaf_id.clone());
                    Some(ResumeRootAgent {
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
                        .ok()
                        .filter(|recovered| !recovered.transcript.is_empty())
                        .map(|recovered| ResumeRootAgent {
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
            Err(_) => None,
        }
    }
}
