//! F-11 guardian auto-review: bounded model review of on-request approvals.
//!
//! The guardian turns an on-request tool approval into a fail-closed model
//! decision: a bounded review model call sees a bounded slice of the session
//! transcript plus the tool request and must answer strict JSON
//! (`{"allow": bool, "reason": string}`). Any deviation — timeout, malformed
//! output, model error — fails the request closed. A per-session circuit
//! breaker trips after consecutive non-accepting outcomes and escalates to
//! the user; any user decision resets it.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use piko_llmd::gateway::{
    InferenceGateway, InferenceItem, InferenceRequest, InvocationContext, ModelRef,
    collect_execution,
};
use piko_protocol::messages::Message;

use crate::api::SessionTreeEntry;

/// Default number of recent transcript entries shown to the reviewer.
pub const DEFAULT_MAX_ENTRIES: usize = 20;
/// Default per-entry character cap for the review context.
pub const DEFAULT_MAX_CHARS_PER_ENTRY: usize = 2_000;
/// Default review deadline in seconds.
pub const DEFAULT_GUARDIAN_TIMEOUT_SECS: u64 = 30;
/// Default circuit-breaker threshold (consecutive non-accepting outcomes).
pub const DEFAULT_MAX_CONSECUTIVE_DENIALS: u32 = 3;

/// Guardian system prompt: the reviewer must answer exactly one JSON object.
pub const REVIEW_PROMPT: &str = r#"You are an automated approval guardian. A tool call in a coding session needs review.

Decide whether the tool call is safe to execute without a human prompt. Failing closed is always safe:
- DENY anything destructive outside the workspace, credential/key access, data exfiltration, or irreversible system changes unless the session context clearly requests it.
- ALLOW routine, low-risk calls that match the session's stated work (builds, tests, reads, formatting, package operations inside the workspace).
- Prefer DENY when uncertain.

Answer with EXACTLY one JSON object and nothing else:
{"allow": true|false, "reason": "one short sentence"}

Do not explain outside the JSON. Do not use markdown. Do not continue the conversation."#;

/// A resolved guardian decision from a strict-JSON review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardianDecision {
    pub allow: bool,
    pub reason: String,
}

/// Input to a guardian review.
#[derive(Debug, Clone)]
pub struct GuardianReviewRequest {
    pub agent_instance_id: String,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
}

/// Resolved guardian behavior for the approval gateway.
#[derive(Debug, Clone)]
pub struct GuardianConfig {
    pub enabled: bool,
    pub timeout: std::time::Duration,
    pub max_consecutive_denials: u32,
}

impl GuardianConfig {
    pub fn from_settings(
        settings: Option<&crate::domain::config::GuardianSettings>,
    ) -> Option<Self> {
        let settings = settings?;
        if !settings.enabled.unwrap_or(false) {
            return None;
        }
        Some(Self {
            enabled: true,
            timeout: std::time::Duration::from_secs(
                settings
                    .timeout_secs
                    .unwrap_or(DEFAULT_GUARDIAN_TIMEOUT_SECS)
                    .max(1),
            ),
            max_consecutive_denials: settings
                .max_consecutive_denials
                .unwrap_or(DEFAULT_MAX_CONSECUTIVE_DENIALS)
                .max(1),
        })
    }
}

/// Parse a strict guardian answer. Any malformed JSON, missing boolean
/// `allow`, or non-string `reason` fails the review closed.
pub fn parse_decision(text: &str) -> Result<GuardianDecision, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|error| format!("guardian output is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "guardian output must be a JSON object".to_string())?;
    let allow = object
        .get("allow")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "guardian output is missing boolean 'allow'".to_string())?;
    let reason = match object.get("reason") {
        Some(serde_json::Value::String(reason)) => reason.clone(),
        Some(_) => return Err("guardian 'reason' must be a string".into()),
        None => String::new(),
    };
    Ok(GuardianDecision { allow, reason })
}

/// Build a bounded review context: the most recent `max_entries` message or
/// context entries of the active branch (post-compaction projection), each
/// truncated to `max_chars_per_entry`.
pub fn build_review_context(
    entries: &[SessionTreeEntry],
    max_entries: usize,
    max_chars_per_entry: usize,
) -> String {
    let context_entries = crate::domain::compaction::context_entries_after_compaction(entries);
    let mut parts: Vec<String> = Vec::new();
    for entry in context_entries.iter().rev().take(max_entries).rev() {
        let role = crate::domain::compaction::entry_role(entry).unwrap_or("metadata");
        let text = crate::domain::compaction::entry_text(entry);
        if text.is_empty() {
            continue;
        }
        let truncated: String = text.chars().take(max_chars_per_entry).collect();
        if text.chars().count() > max_chars_per_entry {
            parts.push(format!("{role}: {truncated}…[truncated]"));
        } else {
            parts.push(format!("{role}: {truncated}"));
        }
    }
    parts.join("\n\n")
}

/// Per-session circuit breaker for the guardian loop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuardianState {
    /// Consecutive non-accepting review outcomes (denies + failures).
    pub consecutive_denials: u32,
    /// True once the breaker tripped; requests escalate to the user.
    pub tripped: bool,
}

impl GuardianState {
    /// Record a non-accepting outcome; trip at the configured threshold.
    pub fn record_non_accept(&mut self, max_consecutive_denials: u32) {
        self.consecutive_denials = self.consecutive_denials.saturating_add(1);
        if self.consecutive_denials >= max_consecutive_denials {
            self.tripped = true;
        }
    }

    /// Reset the breaker (any user decision re-arms the loop).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Host-side review callback wired by the application layer: given a session
/// id and the tool request, run the bounded guardian review and return the
/// parsed decision. `Err` means the review failed and must fail closed.
pub type GuardianReviewCallback = Arc<
    dyn Fn(
            String,
            GuardianReviewRequest,
        ) -> Pin<Box<dyn Future<Output = Result<GuardianDecision, String>> + Send>>
        + Send
        + Sync,
>;

/// Run the bounded guardian model call (compaction-summarizer pattern).
/// Returns the raw model text; the caller parses it with `parse_decision`.
pub async fn run_review(
    model_executor: Arc<dyn InferenceGateway>,
    model: piko_protocol::messages::Model,
    context: String,
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> Result<String, String> {
    let arguments =
        serde_json::to_string_pretty(tool_args).unwrap_or_else(|_| tool_args.to_string());
    let request = format!(
        "Tool call under review: {tool_name}\nArguments:\n{arguments}\n\nRecent session context:\n{context}\n\nReview this tool call."
    );
    let messages = vec![Message::User {
        content: piko_protocol::messages::MessageContent::String(request),
        timestamp: None,
    }];
    let request = InferenceRequest::text_task(
        ModelRef::new(model.provider, model.id),
        REVIEW_PROMPT,
        messages,
        InvocationContext {
            session_id: "guardian".into(),
            agent_instance_id: "guardian".into(),
            root_input_id: "guardian".into(),
            step_id: "review".into(),
            step_message_id: "guardian-review".into(),
        },
    );
    let execution = model_executor
        .start(request, tokio_util::sync::CancellationToken::new())
        .await
        .map_err(|error| error.to_string())?;
    let result = collect_execution(execution)
        .await
        .map_err(|error| error.to_string())?;
    Ok(result
        .items
        .into_iter()
        .filter_map(|item| match item {
            InferenceItem::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_allow_and_deny() {
        let allow = parse_decision(r#"{"allow": true, "reason": "build check"}"#).unwrap();
        assert!(allow.allow);
        assert_eq!(allow.reason, "build check");

        let deny = parse_decision(r#"{"allow": false, "reason": "outside workspace"}"#).unwrap();
        assert!(!deny.allow);
        assert_eq!(deny.reason, "outside workspace");
    }

    #[test]
    fn reason_defaults_to_empty_when_missing() {
        let decision = parse_decision(r#"{"allow": true}"#).unwrap();
        assert!(decision.allow);
        assert!(decision.reason.is_empty());
    }

    #[test]
    fn rejects_malformed_output() {
        for text in [
            "",
            "sure, go ahead",
            r#"{"allow": "yes", "reason": "x"}"#,
            r#"{"reason": "x"}"#,
            r#"{"allow": true, "reason": 42}"#,
            r#"{"allow": true} trailing"#,
            "[1, 2, 3]",
        ] {
            assert!(parse_decision(text).is_err(), "expected error for {text:?}");
        }
    }

    #[test]
    fn breaker_trips_at_threshold_and_resets() {
        let mut state = GuardianState::default();
        state.record_non_accept(3);
        assert!(!state.tripped);
        state.record_non_accept(3);
        assert!(!state.tripped);
        state.record_non_accept(3);
        assert!(state.tripped);
        assert_eq!(state.consecutive_denials, 3);

        state.reset();
        assert_eq!(state, GuardianState::default());
    }

    #[test]
    fn context_is_bounded_by_entries_and_chars() {
        use piko_protocol::MessageEntry;
        let message = |id: &str, parent_id: Option<&str>, text: &str| {
            SessionTreeEntry::Message(MessageEntry {
                id: id.into(),
                parent_id: parent_id.map(str::to_string),
                timestamp: String::new(),
                agent_id: "main".into(),
                agent_instance_id: "root".into(),
                root_input_id: String::new(),
                transcript_seq: 0,
                message: Message::User {
                    content: piko_protocol::messages::MessageContent::String(text.into()),
                    timestamp: None,
                },
            })
        };
        let entries = vec![
            message("a", None, "first"),
            message("b", Some("a"), "second"),
            message("c", Some("b"), &"x".repeat(50)),
        ];
        let context = build_review_context(&entries, 2, 10);
        assert!(!context.contains("first"), "oldest entry should be dropped");
        assert!(context.contains("second"));
        assert!(context.contains("[truncated]"));
        assert!(context.len() < 200);
    }
}
