use std::collections::{BTreeMap, BTreeSet};

use piko_protocol::{
    AgentInboxItem, AgentInstanceIdentity, AgentInstanceLifecycle, AgentRunReport, AgentSpec,
    DurableAgentInput, TodoList,
};
use serde::{Deserialize, Serialize};

use crate::schema::{
    CompactionRecordedV1, EventData, ExecutionStartedV1, RawEvent, SessionForkedV1,
};
use crate::{
    AccountingProjection, DurableCommit, ModelContinuity, Result, StoreError, StoredAgent,
    StoredExecution, StoredMessage, StoredModelStep, StoredTreeEntry,
};

mod transcript;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionAggregate {
    pub revision: u64,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub root: Option<AgentInstanceIdentity>,
    pub selected_agent_instance_id: Option<String>,
    pub name: Option<String>,
    pub selected_tree_entry_id: Option<String>,
    pub root_base_message_id: Option<String>,
    pub messages: BTreeMap<String, StoredMessage>,
    pub tree_entries: BTreeMap<String, StoredTreeEntry>,
    pub agent_heads: BTreeMap<String, String>,
    pub agents: BTreeMap<String, StoredAgent>,
    pub queued_inputs: Vec<DurableAgentInput>,
    pub executions: BTreeMap<String, StoredExecution>,
    #[serde(default)]
    pub model_steps: BTreeMap<String, StoredModelStep>,
    pub inbox: BTreeMap<String, AgentInboxItem>,
    pub compactions: BTreeMap<String, CompactionRecordedV1>,
    pub world_state: Option<serde_json::Value>,
    pub model_continuity: Option<ModelContinuity>,
    pub todo_lists: BTreeMap<String, TodoList>,
    pub fork_origin: Option<SessionForkedV1>,
    pub accounting: AccountingProjection,
    commit_ids: BTreeMap<String, u64>,
    event_ids: BTreeSet<String>,
}

impl SessionAggregate {
    pub fn apply(&mut self, commit: &DurableCommit) -> Result<()> {
        // Transactional apply: clone first so a failed commit leaves `self`
        // untouched. Used by live append preflight.
        let mut next = self.clone();
        next.apply_in_place(commit)?;
        *self = next;
        Ok(())
    }

    /// Replay-only apply: mutates `self` directly, without the transactional
    /// clone. Safe for journal replay where a failed commit aborts the whole
    /// open and the partially-applied aggregate is discarded.
    pub(crate) fn apply_for_replay(&mut self, commit: &DurableCommit) -> Result<()> {
        self.apply_in_place(commit)
    }

    fn apply_in_place(&mut self, commit: &DurableCommit) -> Result<()> {
        crate::schema::validate_extensions("commit", &commit.extensions)?;
        if commit.revision != self.revision + 1 {
            return Err(StoreError::InvalidEvent(format!(
                "expected revision {}, got {}",
                self.revision + 1,
                commit.revision
            )));
        }
        if self.commit_ids.contains_key(&commit.commit_id) {
            return Err(StoreError::IdempotencyConflict(commit.commit_id.clone()));
        }
        for event in &commit.events {
            self.apply_event(commit.revision, event)?;
        }
        self.revision = commit.revision;
        self.updated_at = self.updated_at.max(commit.committed_at);
        self.commit_ids
            .insert(commit.commit_id.clone(), commit.revision);
        Ok(())
    }

    pub fn commit_revision(&self, commit_id: &str) -> Option<u64> {
        self.commit_ids.get(commit_id).copied()
    }

    fn apply_event(&mut self, revision: u64, raw: &RawEvent) -> Result<()> {
        if !self.event_ids.insert(raw.event_id.clone()) {
            return Err(StoreError::IdempotencyConflict(raw.event_id.clone()));
        }
        let Some(event) = raw.decode()? else {
            return Ok(());
        };
        match event {
            EventData::SessionCreated {
                session_id,
                cwd,
                root,
                created_at,
            } => {
                if self.session_id.is_some() || root.session_id != session_id {
                    return Err(StoreError::InvalidEvent(
                        "duplicate or inconsistent session_created".into(),
                    ));
                }
                self.session_id = Some(session_id);
                self.cwd = Some(cwd);
                self.created_at = created_at;
                self.updated_at = created_at;
                self.agents.insert(
                    root.agent_instance_id.clone(),
                    StoredAgent {
                        identity: root.clone(),
                        spec: None,
                        lifecycle: AgentInstanceLifecycle::Open,
                        created_at: 0,
                        changed_at: 0,
                    },
                );
                self.root = Some(root);
            }
            EventData::MessageCommitted(data) => self.apply_message(revision, raw, data)?,
            EventData::BranchSelected {
                selected_tree_entry_id,
                root_base_message_id,
            } => {
                if let Some(base) = &root_base_message_id
                    && !self.messages.contains_key(base)
                {
                    return Err(StoreError::InvalidEvent(format!(
                        "unknown root base message {base}"
                    )));
                }
                self.selected_tree_entry_id = selected_tree_entry_id;
                self.root_base_message_id = root_base_message_id;
            }
            EventData::UsageRecorded(fact) => {
                if Some(fact.attribution.session_id.as_str()) != self.session_id.as_deref() {
                    return Err(StoreError::InvalidEvent(
                        "usage belongs to another session".into(),
                    ));
                }
                self.accounting.record(fact)?;
            }
            EventData::UsageCorrected(correction) => self.accounting.correct(correction)?,
            EventData::SessionMetadataChanged { name } => self.name = name,
            EventData::AgentCreated {
                identity,
                spec,
                created_at,
            } => self.apply_agent_created(identity, spec, created_at)?,
            EventData::AgentLifecycleChanged {
                agent_instance_id,
                lifecycle,
                changed_at,
            } => {
                let agent = self.agent_mut(&agent_instance_id)?;
                agent.lifecycle = lifecycle;
                agent.changed_at = changed_at;
            }
            EventData::AgentInputQueued { input } => self.apply_input_queued(input)?,
            EventData::AgentInputDequeued {
                queued_input_id, ..
            } => {
                let Some(index) = self
                    .queued_inputs
                    .iter()
                    .position(|input| input.queued_input_id == queued_input_id)
                else {
                    return Err(StoreError::InvalidEvent(format!(
                        "unknown queued input {queued_input_id}"
                    )));
                };
                self.queued_inputs.remove(index);
            }
            EventData::ExecutionStarted(started) => self.apply_execution_started(started)?,
            EventData::ExecutionFinished {
                execution_id,
                report,
                finished_at,
            } => self.apply_execution_finished(execution_id, report, finished_at)?,
            EventData::ModelStepCommitted(data) => self.apply_model_step(revision, raw, data)?,
            EventData::InboxReportCommitted { item } => self.apply_inbox_committed(item)?,
            EventData::InboxReportConsumed {
                report_id,
                recipient_agent_instance_id,
                consumed_at,
            } => {
                self.apply_inbox_consumed(&report_id, &recipient_agent_instance_id, consumed_at)?
            }
            EventData::CompactionRecorded(compaction) => {
                if !self
                    .messages
                    .contains_key(&compaction.first_retained_entry_id)
                {
                    return Err(StoreError::InvalidEvent(format!(
                        "unknown retained entry {}",
                        compaction.first_retained_entry_id
                    )));
                }
                if self
                    .compactions
                    .insert(compaction.compaction_id.clone(), compaction)
                    .is_some()
                {
                    return Err(StoreError::InvalidEvent("duplicate compaction".into()));
                }
            }
            EventData::WorldStateAdvanced { facts } => self.world_state = facts,
            EventData::ModelContinuityChanged { provider, model_id } => {
                match (provider, model_id) {
                    (Some(provider), Some(model_id))
                        if !provider.is_empty() && !model_id.is_empty() =>
                    {
                        self.model_continuity = Some(ModelContinuity { provider, model_id });
                    }
                    (None, None) => self.model_continuity = None,
                    _ => {
                        return Err(StoreError::InvalidEvent(
                            "model continuity requires both provider and model".into(),
                        ));
                    }
                }
            }
            EventData::TodoListReplaced {
                agent_instance_id,
                todo_list,
            } => {
                self.agent(&agent_instance_id)?;
                match todo_list {
                    Some(list) if list.agent_instance_id == agent_instance_id => {
                        self.todo_lists.insert(agent_instance_id, list);
                    }
                    Some(_) => {
                        return Err(StoreError::InvalidEvent(
                            "todo list belongs to another agent".into(),
                        ));
                    }
                    None => {
                        self.todo_lists.remove(&agent_instance_id);
                    }
                }
            }
            EventData::SessionForked(origin) => {
                if self.fork_origin.replace(origin).is_some() {
                    return Err(StoreError::InvalidEvent(
                        "duplicate session fork origin".into(),
                    ));
                }
            }
            EventData::TreeEntryRecorded(data) => {
                if data.entry_id.is_empty() || data.entry_type.is_empty() {
                    return Err(StoreError::InvalidEvent(
                        "tree entry requires id and type".into(),
                    ));
                }
                if let Some(parent) = &data.parent_entry_id
                    && !self.tree_entries.contains_key(parent)
                    && !self.messages.contains_key(parent)
                {
                    return Err(StoreError::InvalidEvent(format!(
                        "unknown tree entry parent {parent}"
                    )));
                }
                if self
                    .tree_entries
                    .insert(
                        data.entry_id.clone(),
                        StoredTreeEntry {
                            revision,
                            event_id: raw.event_id.clone(),
                            data,
                        },
                    )
                    .is_some()
                {
                    return Err(StoreError::InvalidEvent("duplicate tree entry".into()));
                }
            }
            EventData::AgentSelected {
                agent_instance_id, ..
            } => {
                self.agent(&agent_instance_id)?;
                self.selected_agent_instance_id = Some(agent_instance_id);
            }
        }
        Ok(())
    }

    fn agent(&self, id: &str) -> Result<&StoredAgent> {
        self.agents
            .get(id)
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown agent {id}")))
    }

    fn agent_mut(&mut self, id: &str) -> Result<&mut StoredAgent> {
        self.agents
            .get_mut(id)
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown agent {id}")))
    }

    fn apply_agent_created(
        &mut self,
        identity: AgentInstanceIdentity,
        spec: AgentSpec,
        created_at: i64,
    ) -> Result<()> {
        if Some(identity.session_id.as_str()) != self.session_id.as_deref() {
            return Err(StoreError::InvalidEvent(
                "agent belongs to another session".into(),
            ));
        }
        if let Some(parent) = &identity.parent_agent_instance_id {
            self.agent(parent)?;
        }
        if let Some(existing) = self.agents.get_mut(&identity.agent_instance_id) {
            if existing.identity != identity || existing.spec.is_some() {
                return Err(StoreError::InvalidEvent("duplicate agent".into()));
            }
            existing.spec = Some(spec);
            existing.created_at = created_at;
            existing.changed_at = created_at;
            return Ok(());
        }
        self.agents.insert(
            identity.agent_instance_id.clone(),
            StoredAgent {
                identity,
                spec: Some(spec),
                lifecycle: AgentInstanceLifecycle::Open,
                created_at,
                changed_at: created_at,
            },
        );
        Ok(())
    }

    fn apply_input_queued(&mut self, input: DurableAgentInput) -> Result<()> {
        self.agent(&input.request.agent_instance_id)?;
        if Some(input.request.session_id.as_str()) != self.session_id.as_deref() {
            return Err(StoreError::InvalidEvent(
                "input belongs to another session".into(),
            ));
        }
        if self
            .queued_inputs
            .iter()
            .any(|existing| existing.queued_input_id == input.queued_input_id)
        {
            return Err(StoreError::InvalidEvent("duplicate queued input".into()));
        }
        self.queued_inputs.push(input);
        Ok(())
    }

    fn apply_execution_started(&mut self, started: ExecutionStartedV1) -> Result<()> {
        self.agent(&started.agent_instance_id)?;
        if self.executions.contains_key(&started.execution_id) {
            return Err(StoreError::InvalidEvent("duplicate execution".into()));
        }
        if self.executions.values().any(|execution| {
            execution.started.agent_instance_id == started.agent_instance_id
                && execution.finished_at.is_none()
        }) {
            return Err(StoreError::InvalidEvent(
                "agent already has an active execution".into(),
            ));
        }
        self.executions.insert(
            started.execution_id.clone(),
            StoredExecution {
                started,
                message_head: None,
                model_step_ids: Vec::new(),
                report: None,
                finished_at: None,
            },
        );
        Ok(())
    }

    fn apply_execution_finished(
        &mut self,
        execution_id: String,
        report: AgentRunReport,
        finished_at: i64,
    ) -> Result<()> {
        let execution = self
            .executions
            .get_mut(&execution_id)
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown execution {execution_id}")))?;
        if execution.finished_at.is_some()
            || report.agent_instance_id != execution.started.agent_instance_id
        {
            return Err(StoreError::InvalidEvent(
                "invalid execution completion".into(),
            ));
        }
        execution.report = Some(report);
        execution.finished_at = Some(finished_at);
        Ok(())
    }

    fn apply_inbox_committed(&mut self, item: AgentInboxItem) -> Result<()> {
        self.agent(&item.recipient_agent_instance_id)?;
        self.agent(&item.source_agent_instance_id)?;
        if item.report.agent_instance_id != item.source_agent_instance_id
            || item.report.report_id != item.report_id
            || item.consumed_at.is_some()
        {
            return Err(StoreError::InvalidEvent("invalid inbox report".into()));
        }
        if self.inbox.insert(item.report_id.clone(), item).is_some() {
            return Err(StoreError::InvalidEvent("duplicate inbox report".into()));
        }
        Ok(())
    }

    fn apply_inbox_consumed(
        &mut self,
        report_id: &str,
        recipient_id: &str,
        consumed_at: i64,
    ) -> Result<()> {
        let item = self
            .inbox
            .get_mut(report_id)
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown inbox report {report_id}")))?;
        if item.recipient_agent_instance_id != recipient_id || item.consumed_at.is_some() {
            return Err(StoreError::InvalidEvent("invalid inbox consumption".into()));
        }
        item.consumed_at = Some(consumed_at);
        Ok(())
    }
}
