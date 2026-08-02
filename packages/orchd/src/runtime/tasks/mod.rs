//! Session-scoped cancellable background work (F-01 / D-01).
//!
//! Long-lived side work spawned by or alongside a turn runs as a typed task
//! with a lifecycle and cancellation, owned by the session so shutdown never
//! orphans it. Kinds are open strings owned by the feature that defines the
//! work (e.g. `compact` with F-05, `review` with F-11); F-01 deliberately
//! introduces no task taxonomy and no new durable surface — each feature
//! persists its results through the channels it already owns (transcript
//! messages, inbox reports).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::runtime::utils::now_ms;

/// A background task's typed result.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskResult {
    pub summary: String,
    pub data: Option<serde_json::Value>,
}

/// Snapshot of one background task's lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSnapshot {
    pub task_id: String,
    /// Feature-owned kind; open string so F-05/F-08/F-11 define their own.
    pub kind: String,
    pub agent_instance_id: String,
    pub status: TaskStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub summary: String,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

struct TaskEntry {
    snapshot: TaskSnapshot,
    cancel: CancellationToken,
}

/// Handle to a spawned background task. Dropping the handle does not cancel
/// the task; cancellation is explicit through the registry.
#[derive(Clone)]
pub struct TaskHandle {
    registry: Arc<TaskRegistry>,
    task_id: String,
    cancel: CancellationToken,
}

impl TaskHandle {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub async fn cancel(&self) -> bool {
        self.registry.cancel(&self.task_id).await
    }
}

/// Session-scoped registry of cancellable background tasks. Session shutdown
/// cancels every running task so no orphaned work survives detach.
pub struct TaskRegistry {
    tasks: Mutex<HashMap<String, TaskEntry>>,
}

impl TaskRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tasks: Mutex::new(HashMap::new()),
        })
    }

    /// Spawn a background task. `work` receives the cancellation token so it
    /// can observe aborts cooperatively; in-flight awaits are also dropped
    /// when the token fires.
    pub async fn spawn<F, Fut>(
        self: &Arc<Self>,
        kind: impl Into<String>,
        agent_instance_id: impl Into<String>,
        summary: impl Into<String>,
        work: F,
    ) -> TaskHandle
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<TaskResult, String>> + Send + 'static,
    {
        let task_id = format!("task_{}", Uuid::new_v4());
        let cancel = CancellationToken::new();
        let running = TaskSnapshot {
            task_id: task_id.clone(),
            kind: kind.into(),
            agent_instance_id: agent_instance_id.into(),
            status: TaskStatus::Running,
            started_at: now_ms(),
            finished_at: None,
            summary: summary.into(),
            result: None,
            error: None,
        };
        self.tasks.lock().await.insert(
            task_id.clone(),
            TaskEntry {
                snapshot: running,
                cancel: cancel.clone(),
            },
        );

        let registry = Arc::clone(self);
        let task_id_for_task = task_id.clone();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            let result = if cancel_for_task.is_cancelled() {
                Err("cancelled".into())
            } else {
                tokio::select! {
                    result = work(cancel_for_task.clone()) => result,
                    _ = cancel_for_task.cancelled() => Err("cancelled".into()),
                }
            };
            registry.finish(&task_id_for_task, result).await;
        });

        TaskHandle {
            registry: Arc::clone(self),
            task_id,
            cancel,
        }
    }

    /// Cancel a running task: abort its work, mark it cancelled, and return
    /// whether a running task was found.
    pub async fn cancel(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        let Some(entry) = tasks.get_mut(task_id) else {
            return false;
        };
        if entry.snapshot.status != TaskStatus::Running {
            return false;
        }
        entry.cancel.cancel();
        entry.snapshot.status = TaskStatus::Cancelled;
        entry.snapshot.finished_at = Some(now_ms());
        entry.snapshot.error = Some("cancelled".into());
        true
    }

    /// Cancel every running task. Called on session shutdown.
    pub async fn cancel_all(&self) {
        let task_ids: Vec<String> = self
            .tasks
            .lock()
            .await
            .iter()
            .filter(|(_, entry)| entry.snapshot.status == TaskStatus::Running)
            .map(|(task_id, _)| task_id.clone())
            .collect();
        for task_id in task_ids {
            let _ = self.cancel(&task_id).await;
        }
    }

    pub async fn snapshots(&self) -> Vec<TaskSnapshot> {
        self.tasks
            .lock()
            .await
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect()
    }

    async fn finish(&self, task_id: &str, result: Result<TaskResult, String>) {
        let mut tasks = self.tasks.lock().await;
        let Some(entry) = tasks.get_mut(task_id) else {
            return;
        };
        if entry.snapshot.status != TaskStatus::Running {
            // Cancelled by the registry; the cancel already won.
            return;
        }
        let snapshot = &mut entry.snapshot;
        snapshot.finished_at = Some(now_ms());
        match result {
            Ok(task_result) => {
                snapshot.status = TaskStatus::Succeeded;
                snapshot.result = Some(task_result);
            }
            Err(error) => {
                snapshot.status = TaskStatus::Failed;
                snapshot.error = Some(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn task_lifecycle_runs_to_succeeded_with_typed_result() {
        let registry = TaskRegistry::new();
        let handle = registry
            .spawn("compact", "root", "compacting", |_cancel| async {
                Ok(TaskResult {
                    summary: "done".into(),
                    data: Some(serde_json::json!({ "tokens": 42 })),
                })
            })
            .await;
        wait_for(&registry, |snapshot| {
            snapshot.status == TaskStatus::Succeeded
        })
        .await;
        let snapshots = registry.snapshots().await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].task_id, handle.task_id());
        assert_eq!(snapshots[0].kind, "compact");
        assert_eq!(snapshots[0].result.as_ref().unwrap().summary, "done");
    }

    #[tokio::test]
    async fn task_failure_records_the_error() {
        let registry = TaskRegistry::new();
        registry
            .spawn("review", "root", "reviewing", |_cancel| async {
                Err::<TaskResult, _>("review failed".into())
            })
            .await;
        wait_for(&registry, |snapshot| snapshot.status == TaskStatus::Failed).await;
        let snapshots = registry.snapshots().await;
        assert_eq!(snapshots[0].error.as_deref(), Some("review failed"));
    }

    #[tokio::test]
    async fn cancel_aborts_work_and_marks_cancelled() {
        let registry = TaskRegistry::new();
        let handle = registry
            .spawn("user_shell", "root", "running", |cancel| async move {
                cancel.cancelled().await;
                Err::<TaskResult, _>("cancelled".into())
            })
            .await;
        assert!(handle.cancel().await);
        wait_for(&registry, |snapshot| {
            snapshot.status == TaskStatus::Cancelled
        })
        .await;
        let snapshots = registry.snapshots().await;
        assert_eq!(snapshots[0].status, TaskStatus::Cancelled);
        assert!(snapshots[0].finished_at.is_some());
    }

    #[tokio::test]
    async fn cancel_all_marks_every_running_task_cancelled() {
        let registry = TaskRegistry::new();
        for _ in 0..3 {
            registry
                .spawn("background", "root", "running", |_cancel| {
                    std::future::pending::<Result<TaskResult, String>>()
                })
                .await;
        }
        registry.cancel_all().await;
        let snapshots = registry.snapshots().await;
        assert_eq!(snapshots.len(), 3);
        assert!(
            snapshots
                .iter()
                .all(|snapshot| snapshot.status == TaskStatus::Cancelled)
        );
    }

    async fn wait_for(registry: &Arc<TaskRegistry>, predicate: impl Fn(&TaskSnapshot) -> bool) {
        for _ in 0..200 {
            if registry.snapshots().await.iter().any(&predicate) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("task did not reach expected state");
    }
}
