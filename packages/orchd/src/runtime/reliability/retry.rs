use piko_protocol::CommitError;

pub(crate) enum CommitFailure {
    Retryable,
    Permanent(CommitError),
}

#[derive(Default)]
pub(crate) struct RetryState {
    attempts: u32,
}

impl RetryState {
    pub fn classify(error: CommitError) -> CommitFailure {
        match error {
            CommitError::IdentityMismatch | CommitError::IdempotencyConflict => {
                CommitFailure::Permanent(error)
            }
            _ => CommitFailure::Retryable,
        }
    }

    pub fn next_delay_ms(&mut self) -> u64 {
        let delay_ms = 50_u64.saturating_mul(1_u64 << self.attempts.min(6));
        self.attempts = self.attempts.saturating_add(1);
        delay_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_is_capped_and_monotonic() {
        let mut retry = RetryState::default();
        let delays = (0..10).map(|_| retry.next_delay_ms()).collect::<Vec<_>>();
        assert_eq!(&delays[..4], &[50, 100, 200, 400]);
        assert_eq!(delays[6], 3_200);
        assert_eq!(delays[9], 3_200);
    }

    #[test]
    fn identity_and_idempotency_failures_are_permanent() {
        assert!(matches!(
            RetryState::classify(CommitError::IdentityMismatch),
            CommitFailure::Permanent(CommitError::IdentityMismatch)
        ));
        assert!(matches!(
            RetryState::classify(CommitError::Unavailable),
            CommitFailure::Retryable
        ));
    }
}
