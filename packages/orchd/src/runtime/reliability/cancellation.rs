use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio_util::sync::CancellationToken;

/// Process-local control plane for cancelling a run even while AgentActor is
/// awaiting startup I/O and cannot drain its mailbox.
pub(crate) struct RunCancellation {
    next_generation: AtomicU64,
    active: Mutex<Option<(u64, String, CancellationToken)>>,
}

impl RunCancellation {
    pub fn new() -> Self {
        Self {
            next_generation: AtomicU64::new(0),
            active: Mutex::new(None),
        }
    }

    pub fn begin(&self, root_input_id: String) -> (u64, CancellationToken) {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let token = CancellationToken::new();
        *self.active.lock().expect("run cancellation lock poisoned") =
            Some((generation, root_input_id, token.clone()));
        (generation, token)
    }

    pub fn cancel_active(&self) -> Option<String> {
        let active = self.active.lock().expect("run cancellation lock poisoned");
        if let Some((_, root_input_id, token)) = active.as_ref() {
            token.cancel();
            Some(root_input_id.clone())
        } else {
            None
        }
    }

    pub fn cancel_active_if_root(&self, root_input_id: &str) -> bool {
        let active = self.active.lock().expect("run cancellation lock poisoned");
        if let Some((_, active_root_input_id, token)) = active.as_ref()
            && active_root_input_id == root_input_id
        {
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
            .is_some_and(|(current, _, _)| *current == generation)
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
        let (old, _) = cancellation.begin("old".into());
        let (_, current) = cancellation.begin("current".into());
        cancellation.finish(old);
        assert_eq!(cancellation.cancel_active().as_deref(), Some("current"));
        assert!(current.is_cancelled());
    }

    #[test]
    fn conditional_cancel_never_targets_a_successor_root() {
        let cancellation = RunCancellation::new();
        let (_, token) = cancellation.begin("current".into());
        assert!(!cancellation.cancel_active_if_root("previous"));
        assert!(!token.is_cancelled());
        assert!(cancellation.cancel_active_if_root("current"));
        assert!(token.is_cancelled());
    }
}
