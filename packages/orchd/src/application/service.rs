use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputDelivery, InputDisposition, InputReceipt, InputSource, SessionCursor,
    SessionRuntimeSnapshot, SubmitTaskInput, SubscribeRequest, TaskControlRequest, TaskHandle,
    TaskSnapshot, TaskStatus, WorkSnapshot, WorkStatus,
};

use crate::api::{AgentApiError, AgentRuntime, SessionSubscription};
use crate::domain::tasks::task::{AgentTask, TaskSource};
use crate::runtime::types::{TaskInputEnvelope, TaskMailboxMessage};

use super::supervisor::Supervisor;
use super::task_driver::spawn_task_driver;
use super::task_launcher::spawn_registered_agent_stream;
use super::task_registry::TaskRegistry;
use super::utils::generate_task_id;

/// Agent API facade over the existing supervisor runtime.
pub struct AgentRuntimeService {
    supervisor: Arc<Supervisor>,
}

impl AgentRuntimeService {
    pub fn new(supervisor: Arc<Supervisor>) -> Self {
        Self { supervisor }
    }

    pub fn from_supervisor(supervisor: &Supervisor) -> Self {
        Self::new(Arc::new(Supervisor::with_state(Arc::clone(
            &supervisor.state,
        ))))
    }

    pub fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    pub fn runtime_for(supervisor: &Supervisor) -> Self {
        Self::from_supervisor(supervisor)
    }

    async fn deliver_input(&self, request: SubmitTaskInput) -> Result<InputReceipt, AgentApiError> {
        if let Some(stored) = self
            .supervisor
            .state
            .registry
            .lookup_input_receipt(&request.task_id, &request.request_id)
            .await
        {
            if stored.input == request {
                return Ok(InputReceipt {
                    disposition: InputDisposition::Duplicate,
                    ..stored.receipt
                });
            }
            return Err(AgentApiError::IdempotencyConflict);
        }

        let handle = self
            .supervisor
            .state
            .registry
            .handle(&request.task_id)
            .await
            .ok_or(AgentApiError::TaskNotFound)?;

        let registered_session = self
            .supervisor
            .state
            .registry
            .task_session(&request.task_id)
            .await;
        if registered_session.is_some_and(|session_id| session_id != request.session_id) {
            return Err(AgentApiError::SessionMismatch);
        }

        let sent = handle
            .control_tx
            .send(TaskMailboxMessage::Input(TaskInputEnvelope {
                input: request.clone(),
            }))
            .is_ok();
        if !sent {
            return Err(AgentApiError::RuntimeUnavailable);
        }

        if matches!(request.delivery, InputDelivery::AfterCurrentStep) {
            self.supervisor
                .state
                .registry
                .clear_task_result(&request.task_id)
                .await;
        }

        let receipt = InputReceipt {
            request_id: request.request_id.clone(),
            task_id: request.task_id.clone(),
            work_id: request.work_id.clone(),
            message_id: request.message_id.clone(),
            disposition: InputDisposition::Accepted,
        };
        self.supervisor
            .state
            .registry
            .record_input_receipt(&request, receipt.clone())
            .await;

        Ok(receipt)
    }

    async fn launch_task(&self, request: CreateTaskRequest) -> Result<TaskHandle, AgentApiError> {
        if let Some(stored) = self
            .supervisor
            .state
            .registry
            .lookup_create_task(&request.request_id)
            .await
        {
            if TaskRegistry::create_requests_match(
                &stored.request,
                &request,
                &stored.handle.task_id,
            ) {
                return Ok(stored.handle);
            }
            return Err(AgentApiError::IdempotencyConflict);
        }

        let task_id = request.task_id.clone().unwrap_or_else(generate_task_id);
        let spec = self.supervisor.ensure_agent(&request.agent_id).await;
        let host_context = request.host_context.clone();
        let session_id = host_context.session_id.clone();

        let task = AgentTask {
            id: Some(task_id.clone()),
            target_agent_id: request.agent_id.clone(),
            prompt: String::new(),
            source: match &request.source {
                InputSource::Task {
                    task_id: parent_task_id,
                    agent_id,
                } => TaskSource::Agent {
                    agent_id: agent_id.clone(),
                    task_id: parent_task_id.clone(),
                },
                _ => TaskSource::User,
            },
            priority: None,
            parent_task_id: request.parent_task_id.clone(),
            history: request.initial_history.clone(),
            host_context: Some(host_context),
        };

        let stream = spawn_registered_agent_stream(
            &self.supervisor,
            spec,
            task,
            matches!(
                request.mode,
                piko_protocol::agent_runtime::TaskMode::Attached
            ),
        )
        .await;
        spawn_task_driver(Arc::clone(&self.supervisor.state), task_id.clone(), stream);

        let handle = TaskHandle {
            session_id,
            task_id,
            agent_id: request.agent_id.clone(),
            status: TaskStatus::Created,
        };
        self.supervisor
            .state
            .registry
            .record_create_task(&request, handle.clone())
            .await;
        Ok(handle)
    }
}

#[async_trait]
impl AgentRuntime for AgentRuntimeService {
    async fn create_task(&self, request: CreateTaskRequest) -> Result<TaskHandle, AgentApiError> {
        self.launch_task(request).await
    }

    async fn submit_input(&self, request: SubmitTaskInput) -> Result<InputReceipt, AgentApiError> {
        self.deliver_input(request).await
    }

    async fn control_task(
        &self,
        request: TaskControlRequest,
    ) -> Result<TaskSnapshot, AgentApiError> {
        let task_id = match &request {
            TaskControlRequest::Close { task_id, .. }
            | TaskControlRequest::Reopen { task_id, .. }
            | TaskControlRequest::CancelWork { task_id, .. }
            | TaskControlRequest::Terminate { task_id, .. } => task_id.clone(),
        };

        let handle = self
            .supervisor
            .state
            .registry
            .handle(&task_id)
            .await
            .ok_or(AgentApiError::TaskNotFound)?;

        match &request {
            TaskControlRequest::Terminate { .. } => handle.cancel.cancel(),
            _ => {
                if handle
                    .control_tx
                    .send(TaskMailboxMessage::Control(request.clone()))
                    .is_err()
                {
                    return Err(AgentApiError::RuntimeUnavailable);
                }
            }
        }

        self.task_snapshot(task_id).await
    }

    async fn task_snapshot(&self, task_id: String) -> Result<TaskSnapshot, AgentApiError> {
        let tasks = self.supervisor.state.registry.tasks_snapshot().await;
        let task = tasks.get(&task_id).ok_or(AgentApiError::TaskNotFound)?;
        let session_id = self
            .supervisor
            .state
            .registry
            .task_session(&task_id)
            .await
            .unwrap_or_else(|| self.supervisor.state.run_id.clone());

        Ok(TaskSnapshot {
            session_id,
            task_id: task.id.clone(),
            agent_id: task.target_agent_id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            status: map_task_status(&task.status),
            active_work: active_work_snapshot(&task.status, &task.id),
        })
    }

    async fn session_snapshot(
        &self,
        session_id: String,
    ) -> Result<SessionRuntimeSnapshot, AgentApiError> {
        let tasks = self.supervisor.state.registry.tasks_snapshot().await;
        let dag = self.supervisor.state.registry.task_dag_snapshot().await;
        let mut snapshots = Vec::new();
        let mut root_task_id = None;

        for (task_id, task) in &tasks {
            if dag
                .get(task_id)
                .and_then(|parent| parent.as_ref())
                .is_none()
            {
                root_task_id.get_or_insert_with(|| task_id.clone());
            }
            snapshots.push(TaskSnapshot {
                session_id: session_id.clone(),
                task_id: task.id.clone(),
                agent_id: task.target_agent_id.clone(),
                parent_task_id: task.parent_task_id.clone(),
                status: map_task_status(&task.status),
                active_work: active_work_snapshot(&task.status, &task.id),
            });
        }

        Ok(SessionRuntimeSnapshot {
            session_id,
            root_task_id: root_task_id.clone(),
            active_task_id: root_task_id,
            tasks: snapshots,
            cursor: SessionCursor {
                epoch: self.supervisor.state.run_id.clone(),
                seq: 0,
            },
        })
    }

    async fn subscribe_session(
        &self,
        request: SubscribeRequest,
    ) -> Result<SessionSubscription, AgentApiError> {
        let hub = self.supervisor.session_hub(&request.session_id).await;
        let cursor = request.after.clone().unwrap_or_else(|| hub.cursor());
        let subscription = hub.subscribe();
        Ok(SessionSubscription {
            session_id: request.session_id,
            cursor: cursor.clone(),
            output: crate::runtime::events::merged_output_stream(subscription, cursor),
        })
    }
}

fn map_task_status(status: &piko_protocol::agents::AgentTaskStatus) -> TaskStatus {
    use piko_protocol::agents::AgentTaskStatus;
    match status {
        AgentTaskStatus::Queued => TaskStatus::Created,
        AgentTaskStatus::Running => TaskStatus::Running,
        AgentTaskStatus::Idle => TaskStatus::Idle,
        AgentTaskStatus::Closed => TaskStatus::Closed,
        AgentTaskStatus::Completed | AgentTaskStatus::Cancelled => TaskStatus::Terminated,
        AgentTaskStatus::Failed => TaskStatus::Failed,
    }
}

fn active_work_snapshot(
    status: &piko_protocol::agents::AgentTaskStatus,
    work_id: &str,
) -> Option<WorkSnapshot> {
    use piko_protocol::agents::AgentTaskStatus;
    match status {
        AgentTaskStatus::Running => Some(WorkSnapshot {
            work_id: work_id.to_string(),
            status: WorkStatus::Running,
        }),
        AgentTaskStatus::Queued => Some(WorkSnapshot {
            work_id: work_id.to_string(),
            status: WorkStatus::Accepted,
        }),
        _ => None,
    }
}
