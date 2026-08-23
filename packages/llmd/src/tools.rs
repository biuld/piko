use crate::capabilities::{ToolExecutionLocus, UpstreamToolKind};
use piko_protocol::messages::UpstreamAction;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UpstreamApprovalPolicy {
    Never,
    Always,
    OnRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticResourceRef {
    pub namespace: String,
    pub resource: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamToolDefinition {
    pub name: String,
    pub kind: UpstreamToolKind,
    pub resources: Vec<SemanticResourceRef>,
    pub approval: UpstreamApprovalPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub enum InferenceTool {
    Caller(piko_protocol::ToolDef),
    Upstream(UpstreamToolDefinition),
    Hybrid {
        caller: piko_protocol::ToolDef,
        upstream: UpstreamToolDefinition,
    },
}

impl InferenceTool {
    pub fn locus(&self) -> ToolExecutionLocus {
        match self {
            Self::Caller(_) => ToolExecutionLocus::Caller,
            Self::Upstream(_) => ToolExecutionLocus::Upstream,
            Self::Hybrid { .. } => ToolExecutionLocus::Hybrid,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Caller(definition) => &definition.name,
            Self::Upstream(definition) => &definition.name,
            Self::Hybrid { caller, .. } => &caller.name,
        }
    }

    pub(crate) fn caller(&self) -> Option<&piko_protocol::ToolDef> {
        match self {
            Self::Caller(definition)
            | Self::Hybrid {
                caller: definition, ..
            } => Some(definition),
            Self::Upstream(_) => None,
        }
    }

    pub(crate) fn upstream_kind(&self) -> Option<&UpstreamToolKind> {
        match self {
            Self::Caller(_) => None,
            Self::Upstream(definition)
            | Self::Hybrid {
                upstream: definition,
                ..
            } => Some(&definition.kind),
        }
    }
}

impl From<piko_protocol::ToolDef> for InferenceTool {
    fn from(value: piko_protocol::ToolDef) -> Self {
        Self::Caller(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamToolActivity {
    pub activity_id: String,
    pub tool_name: String,
    pub kind: UpstreamToolKind,
    pub status: UpstreamActivityStatus,
    /// Provider-echoed arguments for the upstream call (e.g. a web search
    /// `action` with `query`), when the provider surfaces them.
    pub arguments: Option<serde_json::Value>,
    /// Typed, cleaned action for known upstream tools (e.g. a web search
    /// `Search { queries }` / `OpenPage { url }`). Populated from the provider
    /// `action` value at the decode boundary; consumers read it directly.
    pub action: Option<UpstreamAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UpstreamActivityStatus {
    Started,
    InProgress,
    Completed,
    Failed,
}

impl UpstreamActivityStatus {
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
pub struct UpstreamApprovalRequest {
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
    UpstreamActivity(UpstreamToolActivity),
    ApprovalRequired(UpstreamApprovalRequest),
    Source(InferenceSource),
    Citation(InferenceCitation),
    Artifact(GeneratedArtifact),
}
