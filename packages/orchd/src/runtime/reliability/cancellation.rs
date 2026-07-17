use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio_util::sync::CancellationToken;

/// Process-local control plane for cancelling a run even while AgentActor is
/// awaiting startup I/O and cannot drain its mailbox.
pub(crate) struct RunCancellation {
    next_generation: AtomicU64,
    active: Mutex<Option<(u64, CancellationToken)>>,
}

impl RunCancellation {
    pub fn new() -> Self {
        Self {
            next_generation: AtomicU64::new(0),
            active: Mutex::new(None),
        }
    }

    pub fn begin(&self) -> (u64, CancellationToken) {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let token = CancellationToken::new();
        *self.active.lock().expect("run cancellation lock poisoned") =
            Some((generation, token.clone()));
        (generation, token)
    }

    pub fn cancel_active(&self) -> bool {
        let active = self.active.lock().expect("run cancellation lock poisoned");
        if let Some((_, token)) = active.as_ref() {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn finish(&self, generation: u64) {
        let mut active = self.active.lock().expect("run cancellation lock poisoned");
        if active
            .as_ref()
            .is_some_and(|(current, _)| *current == generation)
        {
            active.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_finish_does_not_clear_a_new_run() {
        let cancellation = RunCancellation::new();
        let (old, _) = cancellation.begin();
        let (_, current) = cancellation.begin();
        cancellation.finish(old);
        assert!(cancellation.cancel_active());
        assert!(current.is_cancelled());
    }
}
