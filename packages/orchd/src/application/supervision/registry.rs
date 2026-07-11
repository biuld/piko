use std::collections::{HashMap, HashSet};

use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::handle::ActiveTaskHandle;
use crate::domain::tasks::task::{AgentTask, AgentTaskState, AgentTaskStatus, TaskSource};
use crate::domain::work::TaskReport;
use crate::runtime::task::mailbox::TaskMailboxMessage;
use piko_protocol::TaskEvent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputReceipt, SubmitTaskInput, TaskControlRequest, TaskHandle,
};
use piko_protocol::agent_runtime::{WorkSnapshot, WorkStatus};

#[derive(Clone)]
pub(crate) struct StoredInputReceipt {
    pub(crate) input: SubmitTaskInput,
    pub(crate) receipt: InputReceipt,
}

#[derive(Clone)]
pub(crate) struct StoredCreateTask {
    pub(crate) request: CreateTaskRequest,
    pub(crate) handle: TaskHandle,
}


pub(crate) struct TaskRegistry {
    task_dag: RwLock<HashMap<String, Option<String>>>,
    task_sessions: RwLock<HashMap<String, String>>,
    handles: RwLock<HashMap<String, ActiveTaskHandle>>,
    registered_task_ids: RwLock<HashSet<String>>,
    task_results: Mutex<HashMap<String, TaskReport>>,
    tasks: RwLock<HashMap<String, AgentTaskState>>,
    input_receipts: Mutex<HashMap<(String, String), StoredInputReceipt>>,
    create_tasks: Mutex<HashMap<String, StoredCreateTask>>,
    control_requests: Mutex<HashMap<String, TaskControlRequest>>,
    active_work: RwLock<HashMap<String, WorkSnapshot>>,
}

impl TaskRegistry {
    pub(crate) fn new() -> Self {
        Self {
            task_dag: RwLock::new(HashMap::new()),
            task_sessions: RwLock::new(HashMap::new()),
            handles: RwLock::new(HashMap::new()),
            registered_task_ids: RwLock::new(HashSet::new()),
            task_results: Mutex::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            input_receipts: Mutex::new(HashMap::new()),
            create_tasks: Mutex::new(HashMap::new()),
            control_requests: Mutex::new(HashMap::new()),
            active_work: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) async fn active_work_snapshot(&self, task_id: &str) -> Option<WorkSnapshot> {
        self.active_work.read().await.get(task_id).cloned()
    }

    pub(crate) async fn set_active_work(&self, task_id: &str, snapshot: WorkSnapshot) {
        self.active_work
            .write()
            .await
            .insert(task_id.to_string(), snapshot);
    }

    pub(crate) async fn clear_active_work(&self, task_id: &str) {
        self.active_work.write().await.remove(task_id);
    }

    pub(crate) async fn cancel_active_work(&self, task_id: &str) {
        self.clear_active_work(task_id).await;
        self.with_task_state_mut(task_id, |task| {
            if let Err(error) = crate::domain::tasks::lifecycle::task_idle(task) {
                tracing::error!(%error, "invalid task transition after work cancellation");
            }
        })
        .await;
    }

    pub(crate) async fn lookup_input_receipt(
        &self,
        task_id: &str,
        request_id: &str,
    ) -> Option<StoredInputReceipt> {
        self.input_receipts
            .lock()
            .await
            .get(&(task_id.to_string(), request_id.to_string()))
            .cloned()
    }

    pub(crate) async fn record_input_receipt(
        &self,
        input: &SubmitTaskInput,
        receipt: InputReceipt,
    ) {
        self.input_receipts.lock().await.insert(
            (input.task_id.clone(), input.request_id.clone()),
            StoredInputReceipt {
                input: input.clone(),
                receipt,
            },
        );
    }

    pub(crate) async fn lookup_create_task(&self, request_id: &str) -> Option<StoredCreateTask> {
        self.create_tasks.lock().await.get(request_id).cloned()
    }

    pub(crate) async fn record_create_task(&self, request: &CreateTaskRequest, handle: TaskHandle) {
        self.create_tasks.lock().await.insert(
            request.request_id.clone(),
            StoredCreateTask {
                request: request.clone(),
                handle,
            },
        );
    }

    pub(crate) async fn lookup_control_request(
        &self,
        request_id: &str,
    ) -> Option<TaskControlRequest> {
        self.control_requests.lock().await.get(request_id).cloned()
    }

    pub(crate) async fn record_control_request(&self, request: TaskControlRequest) {
        self.control_requests
            .lock()
            .await
            .insert(control_request_id(&request).to_string(), request);
    }

    pub(crate) fn create_requests_match(
        stored: &CreateTaskRequest,
        incoming: &CreateTaskRequest,
        assigned_task_id: &str,
    ) -> bool {
        fn normalize(request: &CreateTaskRequest, task_id: &str) -> CreateTaskRequest {
            let mut normalized = request.clone();
            normalized.task_id = Some(
                request
                    .task_id
                    .clone()
                    .unwrap_or_else(|| task_id.to_string()),
            );
            normalized
        }

        normalize(stored, assigned_task_id) == normalize(incoming, assigned_task_id)
    }

    pub(crate) async fn active_root_task_for_agent(
        &self,
        agent_id: &str,
        session_id: &str,
    ) -> Option<String> {
        let tasks = self.tasks.read().await;
        let task_sessions = self.task_sessions.read().await;
        tasks
            .values()
            .find(|t| {
                t.target_agent_id == agent_id
                    && task_sessions.get(&t.id).is_some_and(|id| id == session_id)
                    && t.parent_task_id.is_none()
                    && !matches!(
                        t.status,
                        AgentTaskStatus::Completed
                            | AgentTaskStatus::Cancelled
                            | AgentTaskStatus::Closed
                    )
            })
            .map(|t| t.id.clone())
    }

    pub(crate) async fn handle(&self, task_id: &str) -> Option<ActiveTaskHandle> {
        self.handles.read().await.get(task_id).cloned()
    }

    pub(crate) async fn register_runtime(
        &self,
        task: &AgentTask,
        agent_id: &str,
        cancel: CancellationToken,
        control_tx: tokio::sync::mpsc::UnboundedSender<TaskMailboxMessage>,
    ) -> String {
        let task_id = task.id.clone().expect("task id missing");
        let session_id = task
            .host_context
            .as_ref()
            .expect("registered task host context missing")
            .session_id
            .clone();
        self.tasks.write().await.insert(
            task_id.clone(),
            AgentTaskState {
                id: task_id.clone(),
                target_agent_id: agent_id.to_string(),
                prompt: task.prompt.clone(),
                source: task.source.clone(),
                status: AgentTaskStatus::Queued,
                priority: 0,
                parent_task_id: task.parent_task_id.clone(),
                result: None,
                error: None,
            },
        );
        self.task_dag
            .write()
            .await
            .insert(task_id.clone(), task.parent_task_id.clone());
        self.task_sessions
            .write()
            .await
            .insert(task_id.clone(), session_id);
        self.handles
            .write()
            .await
            .insert(task_id.clone(), ActiveTaskHandle { cancel, control_tx });
        self.registered_task_ids
            .write()
            .await
            .insert(task_id.clone());
        task_id
    }

    pub(crate) async fn cleanup_runtime(&self, task_id: &str) {
        self.handles.write().await.remove(task_id);
        self.registered_task_ids.write().await.remove(task_id);
    }

    pub(crate) async fn record_task_result(&self, task_id: &str, report: TaskReport) {
        self.task_results
            .lock()
            .await
            .insert(task_id.to_string(), report);
    }

    pub(crate) async fn task_result(&self, task_id: &str) -> Option<TaskReport> {
        self.task_results.lock().await.get(task_id).cloned()
    }

    pub(crate) async fn clear_task_result(&self, task_id: &str) {
        self.task_results.lock().await.remove(task_id);
    }

    pub(crate) async fn upsert_task_state(&self, task: AgentTaskState) {
        self.tasks.write().await.insert(task.id.clone(), task);
    }

    pub(crate) async fn with_task_state_mut<F>(&self, task_id: &str, f: F)
    where
        F: FnOnce(&mut AgentTaskState),
    {
        if let Some(task) = self.tasks.write().await.get_mut(task_id) {
            f(task);
        }
    }

    pub(crate) async fn tasks_snapshot(&self) -> HashMap<String, AgentTaskState> {
        self.tasks.read().await.clone()
    }

    pub(crate) async fn task_dag_snapshot(&self) -> HashMap<String, Option<String>> {
        self.task_dag.read().await.clone()
    }

    pub(crate) async fn task_session(&self, task_id: &str) -> Option<String> {
        self.task_sessions.read().await.get(task_id).cloned()
    }

    pub(crate) async fn task_sessions_snapshot(&self) -> HashMap<String, String> {
        self.task_sessions.read().await.clone()
    }

    pub(crate) async fn apply_task_event(&self, event: &TaskEvent) {
        match event {
            TaskEvent::Created {
                session_id,
                task_id,
                agent_id,
                parent_task_id,
                source_agent_id,
                prompt,
                ..
            } => {
                let source = match (source_agent_id, parent_task_id) {
                    (Some(agent_id), Some(task_id)) => TaskSource::Agent {
                        agent_id: agent_id.clone(),
                        task_id: task_id.clone(),
                    },
                    _ => TaskSource::User,
                };
                self.upsert_task_state(AgentTaskState {
                    id: task_id.clone(),
                    target_agent_id: agent_id.clone(),
                    prompt: prompt.clone(),
                    source,
                    status: AgentTaskStatus::Queued,
                    priority: 0,
                    parent_task_id: parent_task_id.clone(),
                    result: None,
                    error: None,
                })
                .await;
                self.task_sessions
                    .write()
                    .await
                    .insert(task_id.clone(), session_id.clone());
            }
            TaskEvent::Started { task_id, .. } => {
                self.task_results.lock().await.remove(task_id);
                let active_work = { self.active_work.read().await.get(task_id).cloned() };
                if let Some(work) = active_work {
                    self.set_active_work(
                        task_id,
                        WorkSnapshot {
                            status: WorkStatus::Running,
                            ..work
                        },
                    )
                    .await;
                }
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) = crate::domain::tasks::lifecycle::task_started(task) {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Idle { task_id, .. } => {
                let active_work = { self.active_work.read().await.get(task_id).cloned() };
                if let Some(work) = active_work {
                    self.set_active_work(
                        task_id,
                        WorkSnapshot {
                            status: WorkStatus::Succeeded,
                            ..work
                        },
                    )
                    .await;
                    self.clear_active_work(task_id).await;
                }
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) = crate::domain::tasks::lifecycle::task_idle(task) {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Completed {
                task_id, summary, ..
            } => {
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) =
                        crate::domain::tasks::lifecycle::task_completed(task, summary.clone(), None)
                    {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Failed { task_id, error, .. } => {
                let active_work = { self.active_work.read().await.get(task_id).cloned() };
                if let Some(work) = active_work {
                    self.set_active_work(
                        task_id,
                        WorkSnapshot {
                            status: WorkStatus::Failed,
                            ..work
                        },
                    )
                    .await;
                    self.clear_active_work(task_id).await;
                }
                self.with_task_state_mut(task_id, |task| {
                    if let Err(transition_error) =
                        crate::domain::tasks::lifecycle::task_failed(task, error.clone())
                    {
                        tracing::error!(
                            "apply_task_event: Invalid task transition: {}",
                            transition_error
                        );
                    }
                })
                .await;
            }
            TaskEvent::Cancelled { task_id, .. } => {
                let active_work = { self.active_work.read().await.get(task_id).cloned() };
                if let Some(work) = active_work {
                    self.set_active_work(
                        task_id,
                        WorkSnapshot {
                            status: WorkStatus::Cancelled,
                            ..work
                        },
                    )
                    .await;
                    self.clear_active_work(task_id).await;
                }
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) = crate::domain::tasks::lifecycle::task_cancelled(task, None)
                    {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Closed { task_id, .. } => {
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) = crate::domain::tasks::lifecycle::task_closed(task) {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Reopened { task_id, .. } => {
                self.with_task_state_mut(task_id, |task| {
                    if let Err(error) = crate::domain::tasks::lifecycle::task_reopened(task) {
                        tracing::error!("apply_task_event: Invalid task transition: {}", error);
                    }
                })
                .await;
            }
            TaskEvent::Steered { task_id, .. } => {
                self.task_results.lock().await.remove(task_id);
            }
            TaskEvent::Joined { .. } => {}
        }

        if let Some(report) = agent_report_from_task_event(event) {
            self.record_task_result(event.task_id(), report).await;
        }
    }
}

fn control_request_id(request: &TaskControlRequest) -> &str {
    match request {
        TaskControlRequest::Close { request_id, .. }
        | TaskControlRequest::Reopen { request_id, .. }
        | TaskControlRequest::CancelWork { request_id, .. }
        | TaskControlRequest::Terminate { request_id, .. } => request_id,
    }
}

pub(crate) fn agent_report_from_task_event(event: &TaskEvent) -> Option<TaskReport> {
    match event {
        TaskEvent::Idle {
            task_id,
            summary,
            total_steps,
            ..
        } => Some(TaskReport {
            text: summary.clone(),
            status: "idle".into(),
            total_steps: *total_steps,
            task_id: Some(task_id.clone()),
        }),
        TaskEvent::Completed {
            task_id,
            summary,
            final_status,
            total_steps,
            ..
        } => Some(TaskReport {
            text: summary.clone(),
            status: final_status.clone(),
            total_steps: *total_steps,
            task_id: Some(task_id.clone()),
        }),
        TaskEvent::Failed { task_id, error, .. } => Some(TaskReport {
            text: error.clone(),
            status: "error".into(),
            total_steps: 0,
            task_id: Some(task_id.clone()),
        }),
        TaskEvent::Cancelled { task_id, .. } => Some(TaskReport {
            text: "cancelled".into(),
            status: "cancelled".into(),
            total_steps: 0,
            task_id: Some(task_id.clone()),
        }),
        _ => None,
    }
}
