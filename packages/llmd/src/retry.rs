//! Retry/backoff budget and retryable-error classification for model requests.

use piko_protocol::config::RetryConfig;

/// Exponential backoff delay for retry `retry_index` (1-based), capped at
/// `max_delay_ms` and multiplied by `jitter` (expected in `[0.9, 1.1]`).
pub fn next_delay_ms(base_delay_ms: u64, max_delay_ms: u64, retry_index: u32, jitter: f64) -> u64 {
    let exp = 2u64.saturating_pow(retry_index.saturating_sub(1));
    let base = base_delay_ms.saturating_mul(exp);
    let jittered = (base.min(max_delay_ms) as f64 * jitter) as u64;
    jittered.max(1)
}

/// Immutable retry policy derived from `RetryConfig`.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    enabled: bool,
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
    budget_ms: u64,
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

    /// Delay before the next retry, or `None` when retries are disabled, the
    /// attempt budget is exhausted, or the delay would exceed the total
    /// retry-time budget.
    pub fn delay_for_retry(&self, retries_used: u32, elapsed_ms: u64, jitter: f64) -> Option<u64> {
        if !self.enabled || retries_used >= self.max_retries {
            return None;
        }
        let delay = next_delay_ms(
            self.base_delay_ms,
            self.max_delay_ms,
            retries_used + 1,
            jitter,
        );
        (elapsed_ms.saturating_add(delay) <= self.budget_ms).then_some(delay)
    }
}

/// Mutable per-request retry accounting, shared across the open phase and all
/// mid-stream restarts of one request.
#[derive(Debug, Clone, Copy, Default)]
pub struct RetryState {
    pub retries_used: u32,
    pub elapsed_ms: u64,
}

impl RetryState {
    pub fn record(&mut self, delay_ms: u64) {
        self.retries_used = self.retries_used.saturating_add(1);
        self.elapsed_ms = self.elapsed_ms.saturating_add(delay_ms);
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status.as_u16(),
        408 | 409 | 425 | 429 | 500 | 502 | 503 | 504 | 520..=529
    )
}

/// Classify a genai error as retryable. Structural signals (HTTP status,
/// transport errors, stream breaks) are authoritative; provider wording is a
/// string-matching fallback.
pub fn is_retryable(error: &genai::Error) -> bool {
    let structural = match error {
        genai::Error::HttpError { status, .. } => is_retryable_status(*status),
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => match webc_error {
            genai::webc::Error::ResponseFailedStatus { status, .. } => is_retryable_status(*status),
            genai::webc::Error::Reqwest(err) => err.is_connect() || err.is_timeout(),
            _ => false,
        },
        // A stream error may wrap an HTTP status error (deferred by genai to
        // the first poll); classify the wrapped error when possible. A plain
        // broken/unparseable stream is recoverable by restarting the request.
        genai::Error::WebStream { error, .. } => {
            if let Some(inner) = error.downcast_ref::<genai::Error>() {
                is_retryable(inner)
            } else if let Some(reqwest_err) = error.downcast_ref::<reqwest::Error>() {
                reqwest_err.is_connect() || reqwest_err.is_timeout()
            } else {
                true
            }
        }
        genai::Error::StreamParse { .. } => true,
        _ => false,
    };

    structural || legacy_string_retryable(&error.to_string())
}

/// String-based fallback for transient-error wording not captured
/// structurally by genai.
fn legacy_string_retryable(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("429")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("504")
        || lower.contains("temporarily")
        || lower.contains("transient")
        || lower.contains("server error")
        || lower.contains("internal server error")
        || lower.contains("overloaded")
        || lower.contains("capacity")
}

/// Jitter factor in `[0.9, 1.1]` applied to backoff delays.
pub(crate) fn jitter() -> f64 {
    use rand::Rng;
    rand::thread_rng().gen_range(0.9..=1.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::config::RetryConfig;

    fn config() -> RetryConfig {
        RetryConfig {
            enabled: true,
            max_retries: 3,
            base_delay_ms: 2000,
            max_delay_ms: 30_000,
            budget_ms: 60_000,
        }
    }

    #[test]
    fn backoff_grows_exponentially_and_respects_cap() {
        assert_eq!(next_delay_ms(2000, 30_000, 1, 1.0), 2000);
        assert_eq!(next_delay_ms(2000, 30_000, 2, 1.0), 4000);
        assert_eq!(next_delay_ms(2000, 30_000, 3, 1.0), 8000);
        // Capped: 2^5 * 2000 = 64s > 30s cap.
        assert_eq!(next_delay_ms(2000, 30_000, 6, 1.0), 30_000);
    }

    #[test]
    fn backoff_applies_jitter_within_range() {
        let delays: Vec<u64> = (0..200)
            .map(|_| next_delay_ms(2000, 30_000, 1, 0.9 + 0.2 * rand::random::<f64>()))
            .collect();
        assert!(delays.iter().all(|d| (1800..=2200).contains(d)));
    }

    #[test]
    fn retries_stop_at_max_attempts() {
        let policy = RetryPolicy::from_config(&config());
        assert!(policy.delay_for_retry(0, 0, 1.0).is_some());
        assert!(policy.delay_for_retry(2, 0, 1.0).is_some());
        assert!(policy.delay_for_retry(3, 0, 1.0).is_none());
    }

    #[test]
    fn disabled_retries_never_schedule() {
        let mut cfg = config();
        cfg.enabled = false;
        let policy = RetryPolicy::from_config(&cfg);
        assert!(policy.delay_for_retry(0, 0, 1.0).is_none());
    }

    #[test]
    fn budget_stops_retry_at_boundary() {
        let mut cfg = config();
        cfg.budget_ms = 2000;
        let policy = RetryPolicy::from_config(&cfg);
        // First retry (2s) fits the 2s budget exactly.
        assert_eq!(policy.delay_for_retry(0, 0, 1.0), Some(2000));
        // Second retry (4s) no longer fits.
        assert!(policy.delay_for_retry(1, 2000, 1.0).is_none());
        // With the default 60s budget the second retry fits.
        let policy = RetryPolicy::from_config(&config());
        assert_eq!(policy.delay_for_retry(1, 0, 1.0), Some(4000));
    }

    #[test]
    fn retry_state_records_delays() {
        let mut state = RetryState::default();
        state.record(2000);
        state.record(4000);
        assert_eq!(state.retries_used, 2);
        assert_eq!(state.elapsed_ms, 6000);
    }

    #[test]
    fn retryable_statuses_classified() {
        for code in [408u16, 409, 425, 429, 500, 502, 503, 504, 520, 529] {
            let err = genai::Error::HttpError {
                status: reqwest::StatusCode::from_u16(code).unwrap(),
                canonical_reason: "x".to_string(),
                body: "x".to_string(),
            };
            assert!(is_retryable(&err), "status {code} should be retryable");
        }
    }

    #[test]
    fn non_retryable_statuses_fail_fast() {
        for code in [400u16, 401, 403, 404, 422] {
            let err = genai::Error::HttpError {
                status: reqwest::StatusCode::from_u16(code).unwrap(),
                canonical_reason: "x".to_string(),
                body: "x".to_string(),
            };
            assert!(!is_retryable(&err), "status {code} should not be retryable");
        }
    }

    #[test]
    fn string_fallback_catches_transient_wording() {
        let err = genai::Error::Internal("upstream temporarily unavailable".to_string());
        assert!(is_retryable(&err));
        let err = genai::Error::Internal("invalid api key".to_string());
        assert!(!is_retryable(&err));
    }
}
