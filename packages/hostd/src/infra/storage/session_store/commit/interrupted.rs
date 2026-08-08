use super::*;

impl SessionStore {
    /// Open-time sweep: every Agent execution that never reached a terminal
    /// is marked cancelled with an interrupted report, so no phantom
    /// "running" execution survives a crash.
    pub fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError> {
        self.with_io(|| self.interrupt_incomplete_agent_executions_unlocked())
    }

    fn interrupt_incomplete_agent_executions_unlocked(&self) -> Result<usize, SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        let mut interrupted = 0;
        for execution in manifest.agent_executions.values_mut() {
            if !matches!(
                execution.status,
                piko_protocol::ExecutionStatus::Accepted | piko_protocol::ExecutionStatus::Running
            ) {
                continue;
            }
            // Interrupted runs append the durable model-visible abort marker
            // (stable id, F-01 / D-01) so the next run sees that work may
            // have partially executed.
            let marker_id = piko_protocol::turn_abort_marker_message_id(&execution.execution_id);
            let recovered = self.load_agent(&manifest.session_id, &execution.agent_instance_id)?;
            if !recovered
                .transcript
                .iter()
                .any(|message| message.id == marker_id)
            {
                let entry = CommittedMessage {
                    id: marker_id.clone(),
                    parent_id: recovered.head_message_id.clone(),
                    agent_instance_id: execution.agent_instance_id.clone(),
                    agent_spec_id: recovered.agent_spec_id,
                    execution_id: Some(execution.execution_id.clone()),
                    source_turn_id: execution.source_turn_id.clone(),
                    transcript_seq: recovered.last_transcript_seq + 1,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    message: piko_protocol::turn_abort_marker(&execution.execution_id),
                };
                self.append_record(
                    &execution.agent_instance_id,
                    &AgentShardRecord::Message(entry),
                )?;
                self.advance_root_leaf_under_lock(
                    &execution.agent_instance_id,
                    &marker_id,
                    chrono::Utc::now().timestamp_millis(),
                )?;
            }
            let report = AgentRunReport {
                agent_instance_id: execution.agent_instance_id.clone(),
                report_id: interrupted_report_id(&execution.run_id),
                outcome: piko_protocol::ExecutionOutcome::Cancelled {
                    reason: Some("interrupted during session recovery".into()),
                },
                summary: "Execution interrupted during session recovery".into(),
                usage: Default::default(),
                artifacts: Vec::new(),
            };
            execution.status = piko_protocol::ExecutionStatus::Cancelled;
            execution.report = Some(report.clone());
            execution.finished_at = Some(chrono::Utc::now().timestamp_millis());
            if let Some(agent) = manifest.agents.get_mut(&execution.agent_instance_id) {
                agent.latest_report = Some(report);
            }
            interrupted += 1;
        }
        if interrupted > 0 {
            manifest.agent_revision = manifest.agent_revision.saturating_add(interrupted as u64);
            manifest.updated_at = chrono::Utc::now().timestamp_millis();
            self.store_manifest(&manifest)?;
        }
        Ok(interrupted)
    }
}

fn interrupted_report_id(run_id: &str) -> String {
    piko_orchd_api::stable_internal_id("report", &["interrupted", run_id])
}
