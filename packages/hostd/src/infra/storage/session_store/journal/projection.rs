use std::collections::BTreeMap;

use piko_protocol::{AgentWorkProcessingStatus, SessionTreeEntry};
use piko_session_store::SessionAggregate;

use crate::domain::sessions::SessionModelRef;
use crate::ports::storage_types::{
    AgentProjection, RootInputProjection, SessionProjection, SessionStorageError,
};

use super::SessionStore;

impl SessionStore {
    pub fn load_projection(&self) -> Result<SessionProjection, SessionStorageError> {
        let aggregate =
            piko_session_store::query_current(&self.session_dir).or_else(|_| self.aggregate())?;
        self.project_session(&aggregate)
    }

    pub(crate) fn project_session(
        &self,
        aggregate: &SessionAggregate,
    ) -> Result<SessionProjection, SessionStorageError> {
        let mut normalized = aggregate.clone();
        normalized.rebuild_work_projection();
        let aggregate = &normalized;
        let session_id = aggregate
            .session_id
            .clone()
            .ok_or_else(|| self.invalid("missing session"))?;
        let cwd = aggregate
            .cwd
            .clone()
            .ok_or_else(|| self.invalid("missing cwd"))?;
        let mut entries: Vec<SessionTreeEntry> = aggregate
            .tree_entries
            .values()
            .filter(|stored| {
                !aggregate.messages.contains_key(&stored.data.entry_id)
                    && SessionTreeEntry::recognizes_recorded_type(&stored.data.entry_type)
            })
            .map(|stored| {
                serde_json::from_value::<SessionTreeEntry>(stored.data.payload.clone()).map_err(
                    |error| {
                        self.invalid(format!(
                            "invalid tree entry {}: {error}",
                            stored.data.entry_id
                        ))
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| {
            aggregate
                .tree_entries
                .get(entry.id())
                .map_or(u64::MAX, |stored| stored.revision)
        });

        let agents = aggregate
            .agents
            .iter()
            .map(|(id, stored)| {
                let latest_report = aggregate
                    .agent_inputs
                    .values()
                    .filter(|input| input.input.agent_instance_id == *id)
                    .filter_map(|input| {
                        let processing = input.processing.as_ref()?;
                        Some((processing.finished_at?, processing.report.clone()?))
                    })
                    .max_by_key(|(finished_at, _)| *finished_at)
                    .map(|(_, report)| report);
                (
                    id.clone(),
                    AgentProjection {
                        identity: stored.identity.clone(),
                        spec: stored.spec.clone(),
                        lifecycle: stored.lifecycle,
                        latest_report,
                        todo_list: aggregate.todo_lists.get(id).cloned(),
                        created_at: stored.created_at,
                        updated_at: stored.changed_at,
                    },
                )
            })
            .collect();
        let root_inputs = aggregate
            .agent_inputs
            .values()
            .filter_map(|input| {
                let processing = input.processing.as_ref()?;
                let agent_instance_id = input.input.agent_instance_id.clone();
                let delivered = processing.report.as_ref().is_some_and(|report| {
                    aggregate.inbox.values().any(|item| {
                        item.report_id == report.report_id
                            && Some(item.recipient_agent_instance_id.as_str())
                                == processing.detached_recipient_agent_instance_id.as_deref()
                    })
                });
                let model_steps = aggregate
                    .model_steps
                    .values()
                    .filter(|stored_step| stored_step.data.root_input_id == input.input.input_id)
                    .map(|stored_step| {
                        let data = &stored_step.data;
                        piko_protocol::ModelStepBoundary {
                            session_id: session_id.clone(),
                            root_input_id: data.root_input_id.clone(),
                            agent_instance_id: data.agent_instance_id.clone(),
                            model_step_id: data.model_step_id.clone(),
                            step_index: data.step_index,
                            started_at: data.started_at,
                            finished_at: data.finished_at,
                            outcome: data.outcome,
                            assistant_message_id: data.assistant_message_id.clone(),
                            tool_call_message_ids: data.tool_call_message_ids.clone(),
                        }
                    })
                    .collect();
                Some((
                    input.input.input_id.clone(),
                    RootInputProjection {
                        agent_instance_id,
                        root_input_id: input.input.input_id.clone(),
                        request_id: input.input.request_id.clone(),
                        detached_recipient_agent_instance_id: processing
                            .detached_recipient_agent_instance_id
                            .clone(),
                        detached_report_delivered: delivered,
                        prompt_assembly_version: processing.prompt_assembly_version,
                        prompt_digest: processing.prompt_digest.clone(),
                        status: processing
                            .report
                            .as_ref()
                            .map_or(AgentWorkProcessingStatus::Running, |report| {
                                report.outcome.status()
                            }),
                        started_at: processing.started_at,
                        finished_at: processing.finished_at,
                        report: processing.report.clone(),
                        model_steps,
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let world_state_baseline = aggregate
            .world_state
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| self.invalid(format!("invalid world state: {error}")))?;

        Ok(SessionProjection {
            session_id,
            cwd,
            name: aggregate.name.clone(),
            created_at: aggregate.created_at,
            updated_at: aggregate.updated_at,
            current_leaf_id: aggregate.selected_tree_entry_id.clone(),
            selected_agent_instance_id: aggregate.selected_agent_instance_id.clone().or_else(
                || {
                    aggregate
                        .root
                        .as_ref()
                        .map(|root| root.agent_instance_id.clone())
                },
            ),
            root_agent_instance_id: aggregate
                .root
                .as_ref()
                .map(|root| root.agent_instance_id.clone()),
            journal_revision: aggregate.revision,
            agents,
            agent_inbox: aggregate.inbox.values().cloned().collect(),
            root_inputs,
            agent_input_queue: aggregate.pending_follow_ups(None),
            agent_work: aggregate.agent_work_snapshots(),
            last_model: aggregate
                .model_continuity
                .as_ref()
                .map(|model| SessionModelRef {
                    provider: model.provider.clone(),
                    model_id: model.model_id.clone(),
                }),
            world_state_baseline,
            entries,
        })
    }

    pub(super) fn invalid(&self, message: impl Into<String>) -> SessionStorageError {
        SessionStorageError::Invalid {
            path: self.session_dir().to_path_buf(),
            message: message.into(),
        }
    }
}
