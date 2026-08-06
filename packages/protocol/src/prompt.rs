//! Semantic prompt assembly DTOs shared across hostd, orchd, and llmd.

use serde::{Deserialize, Serialize};

use crate::{AgentInstanceId, AgentSpec, Message, ToolDef};

pub const AGENT_RUN_PROMPT_ASSEMBLY_VERSION: u32 = 5;

/// Instruction authority is independent from rendered message order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstructionAuthority {
    Platform,
    Operator,
    Agent,
    Project,
    User,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentTrust {
    Trusted,
    WorkspaceControlled,
    Untrusted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PromptBlockKind {
    Instruction,
    Context,
    Catalog,
    Environment,
}

/// Stable source identity. It deliberately contains no source body or secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptSource {
    pub kind: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl PromptSource {
    pub fn new(kind: impl Into<String>, locator: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            locator: locator.into(),
            version: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CacheScope {
    GlobalStable,
    OperatorStable,
    AgentStable,
    CatalogStable,
    ResourceSnapshot,
    RunDynamic,
    NoCache,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptBlock {
    pub id: String,
    pub kind: PromptBlockKind,
    pub authority: InstructionAuthority,
    pub trust: ContentTrust,
    pub source: PromptSource,
    pub content: String,
    pub content_digest: String,
    pub cache_scope: CacheScope,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PromptCachePolicy {
    Disabled,
    #[default]
    ProviderDefault,
    Ephemeral,
    Extended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheSegment {
    pub scope: CacheScope,
    pub block_digests: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_digest: Option<String>,
    pub segment_digest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptCachePlan {
    #[serde(default)]
    pub policy: PromptCachePolicy,
    pub prefix_segments: Vec<CacheSegment>,
    pub semantic_prefix_digest: String,
}

/// Host-owned immutable resources captured for one accepted Agent run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptResourceSnapshot {
    #[serde(default)]
    pub blocks: Vec<PromptBlock>,
    /// Retained, data-only world-state Context message for this run
    /// (F-04 slice 2). It is injected into the transcript at run start and
    /// is never rendered as a frozen prompt block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_state: Option<Message>,
    /// Retained file/skill mention Context messages for this run (F-03 / D-27).
    /// Injected after world-state / inter-agent completions and before the
    /// user input, in appearance order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_mentions: Vec<Message>,
    /// Provider prompt-cache policy for this run (F-03 / D-28).
    #[serde(default)]
    pub cache_policy: PromptCachePolicy,
}

/// The canonical prompt value frozen and reused by every Model Step in a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRunPrompt {
    pub blocks: Vec<PromptBlock>,
    pub assembly_version: u32,
    pub source_digest: String,
    pub cache_plan: PromptCachePlan,
}

/// Exact structured tool catalog frozen with the semantic prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedToolCatalog {
    pub tools: Vec<ToolDef>,
    pub digest: String,
    /// Provider/executor identities contributing definitions to this catalog.
    #[serde(default)]
    pub sources: Vec<PromptSource>,
}

impl ResolvedToolCatalog {
    pub fn new(tools: Vec<ToolDef>, digest: impl Into<String>) -> Self {
        let mut sources = tools
            .iter()
            .map(|tool| tool.provenance.clone().with_version(tool.version.clone()))
            .collect::<Vec<_>>();
        sources.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.locator.cmp(&right.locator))
        });
        sources.dedup_by(|left, right| {
            left.kind == right.kind
                && left.locator == right.locator
                && left.version == right.version
        });
        Self {
            tools,
            digest: digest.into(),
            sources,
        }
    }
}

/// Trusted request passed to the host-owned assembler after tool discovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptAssemblyRequest {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub agent_spec: AgentSpec,
    pub resources: PromptResourceSnapshot,
    pub tool_catalog: ResolvedToolCatalog,
}

/// Latest successful production prompt assembly captured for diagnostics.
///
/// This is process-local observational state, not a durable session record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptDebugSnapshot {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub run_prompt: SemanticRunPrompt,
    /// Retained prompt-resource messages in injection order: world-state,
    /// then mentions. Later orchd-owned context is outside this snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_messages: Vec<Message>,
    pub tool_catalog: ResolvedToolCatalog,
    /// Actual llmd model requests produced from this assembly, in step order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_inputs: Vec<ModelInputDebugSnapshot>,
}

/// Provider-neutral request immediately before llmd enters the provider adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInputDebugSnapshot {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub run_id: String,
    pub step_id: String,
    pub provider: String,
    pub model: String,
    pub request: serde_json::Value,
    pub options: serde_json::Value,
}

/// One bounded page from an AgentInstance's append-only rollout transcript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RolloutPage {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub items: Vec<crate::TranscriptCommittedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
