//! Inter-agent completion Context helpers (F-20).

use crate::{
    AgentWorkReport, ContentTrust, ExecutionOutcome, Message, MessageContent, PromptSource,
};

/// Source kind for retained inter-agent completion Context messages (F-20).
pub const AGENT_COMPLETION_SOURCE_KIND: &str = "agent.completion";

/// Max characters retained in the model-visible completion summary.
pub const AGENT_COMPLETION_SUMMARY_MAX_CHARS: usize = 4_000;

/// Stable message id for an inter-agent completion Context (F-20). Shared by
/// run-start injection and recovery so re-inject is idempotent.
pub fn agent_completion_message_id(report_id: &str) -> String {
    format!("{AGENT_COMPLETION_SOURCE_KIND}/{report_id}")
}

/// True when `message` is a retained completion for `report_id`.
pub fn is_agent_completion_message(message: &Message, report_id: &str) -> bool {
    matches!(
        message,
        Message::Context {
            source,
            ..
        } if source.kind == AGENT_COMPLETION_SOURCE_KIND && source.locator == report_id
    )
}

/// Data-only Context message describing a detached child completion for the
/// parent transcript (F-20). Trust is runtime-sourced; content is not
/// instruction authority.
pub fn agent_completion_context_message(report: &AgentWorkReport) -> Message {
    Message::Context {
        content: MessageContent::String(agent_completion_content(report)),
        trust: ContentTrust::Trusted,
        source: PromptSource::new(AGENT_COMPLETION_SOURCE_KIND, &report.report_id),
        timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        ),
    }
}

/// Pure formatter for the model-visible completion body (stable key order).
pub fn agent_completion_content(report: &AgentWorkReport) -> String {
    let outcome = match &report.outcome {
        ExecutionOutcome::Succeeded { .. } => "succeeded",
        ExecutionOutcome::Failed { .. } => "failed",
        ExecutionOutcome::Cancelled { .. } => "cancelled",
    };
    let summary = match &report.outcome {
        ExecutionOutcome::Failed { error } if !error.is_empty() => bound_completion_summary(error),
        ExecutionOutcome::Cancelled {
            reason: Some(reason),
        } if !reason.is_empty() => bound_completion_summary(reason),
        _ => bound_completion_summary(&report.summary),
    };
    let mut lines = vec![
        "inter-agent completion:".to_string(),
        format!("source_agent_instance_id: {}", report.agent_instance_id),
        format!("report_id: {}", report.report_id),
        format!("outcome: {outcome}"),
    ];
    if !summary.is_empty() {
        lines.push(format!("summary: {summary}"));
    }
    lines.join("\n")
}

fn bound_completion_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let count = trimmed.chars().count();
    if count <= AGENT_COMPLETION_SUMMARY_MAX_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed
        .chars()
        .take(AGENT_COMPLETION_SUMMARY_MAX_CHARS.saturating_sub(1))
        .collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;

    fn report(summary: &str, outcome: ExecutionOutcome) -> AgentWorkReport {
        AgentWorkReport {
            agent_instance_id: "agent_child".into(),
            root_input_id: "input-42".into(),
            report_id: "report-42".into(),
            outcome,
            summary: summary.into(),
            usage: Usage::default(),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn completion_content_lists_facts_in_fixed_order() {
        let body = agent_completion_content(&report(
            "child finished",
            ExecutionOutcome::Succeeded {
                usage: Usage::default(),
            },
        ));
        assert_eq!(
            body,
            "inter-agent completion:\n\
             source_agent_instance_id: agent_child\n\
             report_id: report-42\n\
             outcome: succeeded\n\
             summary: child finished"
        );
        assert_eq!(
            agent_completion_message_id("report-42"),
            "agent.completion/report-42"
        );
    }

    #[test]
    fn failed_outcome_prefers_error_text_over_summary() {
        let body = agent_completion_content(&report(
            "unused",
            ExecutionOutcome::Failed {
                error: "tool boom".into(),
            },
        ));
        assert!(body.contains("outcome: failed"));
        assert!(body.contains("summary: tool boom"));
        assert!(!body.contains("unused"));
    }

    #[test]
    fn summary_truncates_to_max_chars() {
        let long = "x".repeat(AGENT_COMPLETION_SUMMARY_MAX_CHARS + 50);
        let body = agent_completion_content(&report(
            &long,
            ExecutionOutcome::Succeeded {
                usage: Usage::default(),
            },
        ));
        let summary = body
            .lines()
            .find_map(|line| line.strip_prefix("summary: "))
            .expect("summary line");
        assert_eq!(summary.chars().count(), AGENT_COMPLETION_SUMMARY_MAX_CHARS);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn is_agent_completion_message_matches_source_identity() {
        let message = agent_completion_context_message(&report(
            "ok",
            ExecutionOutcome::Succeeded {
                usage: Usage::default(),
            },
        ));
        assert!(is_agent_completion_message(&message, "report-42"));
        assert!(!is_agent_completion_message(&message, "other"));
    }
}
