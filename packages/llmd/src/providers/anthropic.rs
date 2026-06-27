use piko_protocol::model::{InputModality, ModelSummary};

use super::Provider;

// ============================================================================
// AnthropicProvider — built-in Anthropic Claude provider
// ============================================================================

pub struct AnthropicProvider;

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self
    }

    /// Static model catalog — mirrors pi-mono's anthropic.models.ts.
    pub fn models() -> Vec<ModelSummary> {
        vec![
            // ---- Claude Sonnet 4.x ----
            ModelSummary {
                id: "claude-sonnet-4-5-20250929".into(),
                name: "Claude Sonnet 4.5".into(),
                reasoning: true,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 64_000,
            },
            ModelSummary {
                id: "claude-sonnet-4-20250514".into(),
                name: "Claude Sonnet 4".into(),
                reasoning: true,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 64_000,
            },
            // ---- Claude Sonnet 3.x ----
            ModelSummary {
                id: "claude-3-5-sonnet-20241022".into(),
                name: "Claude Sonnet 3.5 v2".into(),
                reasoning: false,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 8_192,
            },
            // ---- Claude Opus 4.x ----
            ModelSummary {
                id: "claude-opus-4-8".into(),
                name: "Claude Opus 4.8".into(),
                reasoning: true,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 1_000_000,
                max_tokens: 128_000,
            },
            ModelSummary {
                id: "claude-opus-4-5-20251101".into(),
                name: "Claude Opus 4.5".into(),
                reasoning: true,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 64_000,
            },
            // ---- Claude Haiku 4.x ----
            ModelSummary {
                id: "claude-haiku-4-5-20251001".into(),
                name: "Claude Haiku 4.5".into(),
                reasoning: true,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 64_000,
            },
            // ---- Claude Opus 3.x ----
            ModelSummary {
                id: "claude-3-opus-20240229".into(),
                name: "Claude Opus 3".into(),
                reasoning: false,
                input: vec![InputModality::Text, InputModality::Image],
                context_window: 200_000,
                max_tokens: 4_096,
            },
        ]
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn adapter_kind(&self) -> genai::adapter::AdapterKind {
        genai::adapter::AdapterKind::Anthropic
    }

    fn list_models(&self) -> Vec<ModelSummary> {
        Self::models()
    }
}
