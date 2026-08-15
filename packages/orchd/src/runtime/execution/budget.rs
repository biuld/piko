use piko_orchd_api::AgentApiError;
use piko_protocol::{SemanticRunPrompt, ToolDef};

use crate::domain::transcript::{TranscriptSnapshot, serialized_tokens};

/// Maximum tokens reserved for a single model step's output.
///
/// Reasoning and completion share one provider-side output budget, so a
/// reasoning model must not double the reserve. Reserving the full
/// `max_output_tokens` (e.g. 384K on deepseek-v4-flash) would consume most
/// of a 1M-token window before any transcript exists (F-35 / ADR-020).
const OUTPUT_RESERVE_CAP: u64 = 32_768;

/// Fail-closed provider preflight (F-04 / D-04). `transcript` must be the
/// exact normalized model view that will be dispatched, so the estimate and
/// the request can never diverge.
pub(super) fn enforce_context_budget(
    prompt: &SemanticRunPrompt,
    transcript: &TranscriptSnapshot,
    tools: &[ToolDef],
    context_window: u64,
    output_reserve: u64,
    reasoning_enabled: bool,
) -> Result<BudgetEstimate, AgentApiError> {
    let prompt_tokens = serialized_tokens(prompt);
    let tool_tokens = serialized_tokens(tools).saturating_add(tools.len() as u64 * 32);
    let output_reserve = output_reserve.min(OUTPUT_RESERVE_CAP);
    // Reasoning output counts against the same completion budget as the
    // final answer, so no separate reasoning reserve is added (F-35 /
    // ADR-020). `reasoning_enabled` is still surfaced in the failure message
    // so a reasoning model that exhausts the budget is diagnosable.
    let reasoning_reserve = 0;
    let safety_margin = (context_window / 50).max(256);
    let fixed_tokens = prompt_tokens
        .saturating_add(tool_tokens)
        .saturating_add(output_reserve)
        .saturating_add(reasoning_reserve)
        .saturating_add(safety_margin);
    if fixed_tokens >= context_window {
        return Err(AgentApiError::ContextBudgetExceeded(format!(
            "fixed estimate prompt={prompt_tokens}, tools={tool_tokens}, output={output_reserve}, reasoning={reasoning_reserve}, reasoning_enabled={reasoning_enabled}, margin={safety_margin}, window={context_window}"
        )));
    }

    let transcript_tokens = transcript.total_tokens();
    let total = fixed_tokens.saturating_add(transcript_tokens);
    if total > context_window {
        return Err(AgentApiError::ContextBudgetExceeded(format!(
            "estimated request={total}, fixed={fixed_tokens}, transcript={transcript_tokens}, context_remaining={}, window={context_window}; compaction required",
            context_window.saturating_sub(total)
        )));
    }
    Ok(BudgetEstimate {
        fixed_tokens,
        transcript_tokens,
        total,
        context_remaining: context_window.saturating_sub(total),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BudgetEstimate {
    pub fixed_tokens: u64,
    pub transcript_tokens: u64,
    pub total: u64,
    pub context_remaining: u64,
}
