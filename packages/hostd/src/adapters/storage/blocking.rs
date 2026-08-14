use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tokio::sync::Semaphore;

use crate::ports::storage_types::SessionStorageError;

const STORAGE_BLOCKING_LIMIT: usize = 8;

#[derive(Debug, Clone)]
pub(super) struct StorageBlockingPool {
    permits: Arc<Semaphore>,
}

impl Default for StorageBlockingPool {
    fn default() -> Self {
        static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
        Self {
            permits: Arc::clone(
                PERMITS.get_or_init(|| Arc::new(Semaphore::new(STORAGE_BLOCKING_LIMIT))),
            ),
        }
    }
}

impl StorageBlockingPool {
    #[cfg(test)]
    fn with_limit(limit: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(limit)),
        }
    }

    pub(super) async fn run<T, F>(&self, operation: F) -> Result<T, SessionStorageError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, SessionStorageError> + Send + 'static,
    {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|error| worker_error(format!("storage concurrency gate closed: {error}")))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .map_err(|error| worker_error(format!("storage blocking worker failed: {error}")))?
    }
}

fn worker_error(message: String) -> SessionStorageError {
    SessionStorageError::Invalid {
        path: PathBuf::from("storage blocking worker"),
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    async fn wait_until(flag: &AtomicBool) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while !flag.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("flag should be set");
    }

    #[tokio::test]
    async fn limit_is_held_for_the_whole_blocking_operation() {
        let pool = StorageBlockingPool::with_limit(1);
        let first_started = Arc::new(AtomicBool::new(false));
        let first_release = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&first_started);
        let worker_release = Arc::clone(&first_release);
        let first_pool = pool.clone();
        let first = tokio::spawn(async move {
            first_pool
                .run(move || {
                    worker_started.store(true, Ordering::Release);
                    while !worker_release.load(Ordering::Acquire) {
                        std::thread::yield_now();
                    }
                    Ok::<_, SessionStorageError>(())
                })
                .await
        });
        wait_until(&first_started).await;

        let second_started = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&second_started);
        let second = tokio::spawn(async move {
            pool.run(move || {
                worker_started.store(true, Ordering::Release);
                Ok::<_, SessionStorageError>(())
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !second_started.load(Ordering::Acquire),
            "second operation must wait for the shared permit"
        );

        first_release.store(true, Ordering::Release);
        first.await.unwrap().unwrap();
        wait_until(&second_started).await;
        second.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancelling_waiter_does_not_cancel_started_filesystem_work() {
        let pool = StorageBlockingPool::with_limit(1);
        let started = Arc::new(AtomicBool::new(false));
        let completed = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&started);
        let worker_completed = Arc::clone(&completed);
        let worker_release = Arc::clone(&release);
        let task = tokio::spawn(async move {
            pool.run(move || {
                worker_started.store(true, Ordering::Release);
                while !worker_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                worker_completed.store(true, Ordering::Release);
                Ok::<_, SessionStorageError>(())
            })
            .await
        });
        wait_until(&started).await;
        task.abort();
        release.store(true, Ordering::Release);
        wait_until(&completed).await;
    }
}
