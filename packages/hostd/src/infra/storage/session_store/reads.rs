use std::fs::{self, File};
use std::io::{BufRead, BufReader};

use piko_protocol::{AgentInboxItem, Message};

use super::super::SessionStorageError;
use super::SessionStore;
use super::io::read_records;
use super::types::*;

impl SessionStore {
    pub fn root_agent_report_for_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::AgentRunReport>, SessionStorageError> {
        let manifest = self.load_manifest()?;
        let Some(root_agent_instance_id) = manifest.root_agent_instance_id else {
            return Ok(None);
        };
        Ok(manifest
            .agent_executions
            .into_values()
            .find(|run| {
                run.agent_instance_id == root_agent_instance_id
                    && run.source_turn_id.as_deref() == Some(turn_id)
            })
            .and_then(|run| run.report))
    }

    pub fn agent_instances(&self) -> Result<Vec<AgentManifestEntry>, SessionStorageError> {
        Ok(self.load_manifest()?.agents.into_values().collect())
    }

    pub fn agent_inbox(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<AgentInboxItem>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_inbox
            .into_iter()
            .filter(|item| item.recipient_agent_instance_id == agent_instance_id)
            .collect())
    }

    pub fn agent_transcript(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<Vec<Message>, SessionStorageError> {
        let recovered = self.load_agent(session_id, agent_instance_id)?;
        Ok(recovered
            .transcript
            .into_iter()
            .map(|message| message.message)
            .collect())
    }

    pub fn agent_execution_reports(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<orchd_api::RecoveredExecutionReport>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_executions
            .into_values()
            .filter(|execution| execution.agent_instance_id == agent_instance_id)
            .filter_map(|execution| {
                Some(orchd_api::RecoveredExecutionReport {
                    internal_execution_id: execution.execution_id,
                    report: execution.report?,
                })
            })
            .collect())
    }

    pub fn agent_queued_inputs(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<piko_protocol::DurableAgentInput>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_input_queue
            .into_iter()
            .filter(|input| input.request.agent_instance_id == agent_instance_id)
            .collect())
    }

    pub fn pending_detached_deliveries(
        &self,
        source_agent_instance_id: &str,
    ) -> Result<Vec<orchd_api::RecoveredDetachedDelivery>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_executions
            .into_values()
            .filter(|run| {
                run.agent_instance_id == source_agent_instance_id && !run.detached_report_delivered
            })
            .filter_map(|run| {
                Some(orchd_api::RecoveredDetachedDelivery {
                    recipient_agent_instance_id: run.detached_recipient_agent_instance_id?,
                    report: run.report?,
                })
            })
            .collect())
    }
    /// Load the full recovered transcript + head for one AgentInstance shard.
    pub fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError> {
        let path = self.agent_path(agent_instance_id);
        let records = read_records(&path)?;
        let Some(AgentShardRecord::Header(header)) = records.first().cloned() else {
            return Err(SessionStorageError::Invalid {
                path,
                message: "missing agent shard header".into(),
            });
        };
        if header.session_id != session_id || header.agent_instance_id != agent_instance_id {
            return Err(SessionStorageError::Invalid {
                path,
                message: "agent shard identity mismatch".into(),
            });
        }
        let mut transcript = Vec::new();
        let mut last_transcript_seq = 0;
        for record in records.into_iter().skip(1) {
            match record {
                AgentShardRecord::Message(message) => {
                    if message.agent_instance_id != agent_instance_id {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: "message identity mismatch".into(),
                        });
                    }
                    let seq = message.transcript_seq;
                    if seq != last_transcript_seq + 1 {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: format!(
                                "invalid transcript sequence: expected {}, got {seq}",
                                last_transcript_seq + 1
                            ),
                        });
                    }
                    last_transcript_seq = seq;
                    transcript.push(message);
                }
                AgentShardRecord::Header(_) => {
                    return Err(SessionStorageError::Invalid {
                        path: path.clone(),
                        message: "duplicate agent shard header".into(),
                    });
                }
            }
        }
        let head_message_id = transcript.last().map(|message| message.id.clone());
        Ok(RecoveredAgent {
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            agent_spec_id: header.agent_spec_id,
            transcript,
            head_message_id,
            last_transcript_seq,
        })
    }

    pub fn next_transcript_seq(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<u64, SessionStorageError> {
        Ok(self
            .load_agent(session_id, agent_instance_id)?
            .last_transcript_seq
            + 1)
    }

    pub fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        let recovered = self.load_agent(session_id, agent_instance_id)?;
        Ok(recovered
            .transcript
            .into_iter()
            .find(|message| message.id == message_id))
    }

    /// Scan an agent shard for a message without requiring the full shard to
    /// parse. Used by the observation path when concurrent appends may leave
    /// a trailing partial line.
    pub fn find_committed_message_lenient(
        &self,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Option<CommittedMessage> {
        let path = self.agent_path(agent_instance_id);
        let file = File::open(&path).ok()?;
        for line in BufReader::new(file).lines() {
            let line = line.ok()?;
            if line.trim().is_empty() {
                continue;
            }
            let record = match serde_json::from_str::<AgentShardRecord>(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if let AgentShardRecord::Message(message) = record
                && message.id == message_id
                && message.agent_instance_id == agent_instance_id
            {
                return Some(message);
            }
        }
        None
    }

    /// List AgentInstance ids with a durable shard under `agents/`.
    pub fn list_agents(&self, session_id: &str) -> Result<Vec<String>, SessionStorageError> {
        let mut agents = Vec::new();
        if !self.agents_dir().exists() {
            return Ok(agents);
        }
        for entry in fs::read_dir(self.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: self.agents_dir(),
            source,
        })? {
            let entry = entry.map_err(|source| SessionStorageError::Io {
                path: self.agents_dir(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
                let header = self.read_header(&path)?;
                if header.session_id != session_id {
                    return Err(SessionStorageError::Invalid {
                        path,
                        message: "agent shard belongs to another session".into(),
                    });
                }
                agents.push(header.agent_instance_id);
            }
        }
        agents.sort();
        Ok(agents)
    }
}
