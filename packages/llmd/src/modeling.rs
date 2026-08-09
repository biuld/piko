use std::fmt;

use piko_protocol::model::ProviderAuthMethod;
use serde::{Deserialize, Serialize};

/// Provider-scoped model identity. A bare model ID is never a target key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelKey {
    pub provider: String,
    pub model: String,
}

impl ModelKey {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn lookup_id(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

impl fmt::Display for ModelKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

/// Wire grammar implemented by an llmd protocol adapter.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    Responses,
    ChatCompletions,
}

impl ProtocolKind {
    pub fn adapter_id(self) -> &'static str {
        match self {
            Self::Responses => "openai_responses",
            Self::ChatCompletions => "openai_chat_completions",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesContinuationPolicy {
    #[default]
    PreviousResponseId,
    EncryptedReasoning,
    StatelessReplay,
}

/// Closed protocol configuration. Protocol-specific policy cannot be attached
/// to a different protocol variant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum ProtocolProfile {
    Responses {
        #[serde(default)]
        continuation: ResponsesContinuationPolicy,
    },
    ChatCompletions,
}

impl ProtocolProfile {
    pub fn kind(self) -> ProtocolKind {
        match self {
            Self::Responses { .. } => ProtocolKind::Responses,
            Self::ChatCompletions => ProtocolKind::ChatCompletions,
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::Responses { .. } => "responses",
            Self::ChatCompletions => "chat/completions",
        }
    }

    pub fn responses_continuation(self) -> Option<ResponsesContinuationPolicy> {
        match self {
            Self::Responses { continuation } => Some(continuation),
            Self::ChatCompletions => None,
        }
    }
}

/// Named API product surface owned by a provider catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiSurface {
    pub id: String,
    pub base_url: String,
    pub auth_methods: Vec<ProviderAuthMethod>,
}

/// Catalog binding from a model to one named API surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTargetProfile {
    pub api_surface: String,
    pub protocol: ProtocolProfile,
}

/// One model target selected for a concrete authentication route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelTarget {
    pub id: String,
    pub model: ModelKey,
    pub api_surface: String,
    pub auth_method: ProviderAuthMethod,
    pub base_url: String,
    pub protocol: ProtocolProfile,
}
