use crate::capabilities::{HostedToolKind, ToolExecutionLocus};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedApprovalPolicy {
    Never,
    Always,
    OnRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticResourceRef {
    pub namespace: String,
    pub resource: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedToolDefinition {
    pub name: String,
    pub kind: HostedToolKind,
    pub resources: Vec<SemanticResourceRef>,
    pub approval: HostedApprovalPolicy,
    /// Host-owned authorization for provider-side execution. Catalog support
    /// alone is never sufficient to dispatch a hosted tool.
    pub authorization: Option<HostedExecutionAuthorization>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedExecutionAuthorization {
    pub approval_id: String,
}

#[derive(Debug, Clone)]
pub enum InferenceTool {
    Caller(piko_protocol::ToolDef),
    Hosted(HostedToolDefinition),
    Hybrid {
        caller: piko_protocol::ToolDef,
        hosted: HostedToolDefinition,
    },
}

impl InferenceTool {
    pub fn locus(&self) -> ToolExecutionLocus {
        match self {
            Self::Caller(_) => ToolExecutionLocus::Caller,
            Self::Hosted(_) => ToolExecutionLocus::Provider,
            Self::Hybrid { .. } => ToolExecutionLocus::Hybrid,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Caller(definition) => &definition.name,
            Self::Hosted(definition) => &definition.name,
            Self::Hybrid { caller, .. } => &caller.name,
        }
    }

    pub(crate) fn caller(&self) -> Option<&piko_protocol::ToolDef> {
        match self {
            Self::Caller(definition)
            | Self::Hybrid {
                caller: definition, ..
            } => Some(definition),
            Self::Hosted(_) => None,
        }
    }

    pub(crate) fn hosted_kind(&self) -> Option<HostedToolKind> {
        match self {
            Self::Caller(_) => None,
            Self::Hosted(definition)
            | Self::Hybrid {
                hosted: definition, ..
            } => Some(definition.kind),
        }
    }
}

impl From<piko_protocol::ToolDef> for InferenceTool {
    fn from(value: piko_protocol::ToolDef) -> Self {
        Self::Caller(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostedToolActivity {
    pub activity_id: String,
    pub tool_name: String,
    pub kind: HostedToolKind,
    pub status: HostedActivityStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HostedActivityStatus {
    Started,
    InProgress,
    Completed,
    Failed,
}

impl HostedActivityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostedApprovalRequest {
    pub approval_id: String,
    pub tool_name: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InferenceSource {
    pub source_id: String,
    pub title: Option<String>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InferenceCitation {
    pub source_id: String,
    pub output_item_id: crate::gateway::OutputItemId,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedArtifact {
    pub artifact_id: String,
    pub media_type: String,
    pub resource: SemanticResourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceAuxiliary {
    HostedActivity(HostedToolActivity),
    ApprovalRequired(HostedApprovalRequest),
    Source(InferenceSource),
    Citation(InferenceCitation),
    Artifact(GeneratedArtifact),
}
