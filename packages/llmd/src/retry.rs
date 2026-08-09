use piko_protocol::config::RetryConfig;
use rand::Rng;

use crate::gateway::InferenceError;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub budget_ms: u64,
}

impl RetryPolicy {
    pub fn from_config(config: &RetryConfig) -> Self {
        Self {
            enabled: config.enabled,
            max_retries: config.max_retries,
            base_delay_ms: config.base_delay_ms,
            max_delay_ms: config.max_delay_ms,
            budget_ms: config.budget_ms,
        }
    }

    pub fn delay_for_retry(&self, retries_used: u32, elapsed_ms: u64, jitter: f64) -> Option<u64> {
        if !self.enabled || retries_used >= self.max_retries {
            return None;
        }
        let exponential = self
            .base_delay_ms
            .saturating_mul(2_u64.saturating_pow(retries_used));
        let delay = ((exponential.min(self.max_delay_ms) as f64) * jitter.clamp(0.5, 1.5)) as u64;
        (elapsed_ms.saturating_add(delay) <= self.budget_ms).then_some(delay)
    }
}

#[derive(Debug, Default)]
pub struct RetryState {
    pub retries_used: u32,
    pub elapsed_ms: u64,
}

impl RetryState {
    pub fn record(&mut self, delay_ms: u64) {
        self.retries_used += 1;
        self.elapsed_ms = self.elapsed_ms.saturating_add(delay_ms);
    }
}

pub fn is_retryable(error: &InferenceError) -> bool {
    error.is_retryable()
}

pub fn jitter() -> f64 {
    rand::thread_rng().gen_range(0.8..=1.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{ErrorClass, InferenceError};

    #[test]
    fn retryable_statuses_are_structural() {
        for status in [408, 409, 425, 429, 500, 502, 503, 504] {
            let mut error = InferenceError::new(ErrorClass::Upstream, "t", "http", "safe");
            error.status = Some(status);
            assert!(is_retryable(&error), "status {status}");
        }
        for status in [400, 401, 403, 404, 422] {
            let mut error = InferenceError::new(ErrorClass::Upstream, "t", "http", "safe");
            error.status = Some(status);
            assert!(!is_retryable(&error), "status {status}");
        }
    }
}
