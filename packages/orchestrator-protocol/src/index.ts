// ---- piko-orchestrator-protocol — public API ----
// Stable, serializable, Host-visible protocol types.

export type {
  AgentArtifact,
  AgentConcurrencyPolicy,
  AgentRuntimeState,
  AgentSpec,
  AgentStatus,
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentTaskState,
  AgentTaskStatus,
  TaskSource,
} from "./agents.js";
export type {
  ApprovalGateway,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "./approval.js";
export type {
  OrchestratorCommand,
  OrchestratorResponse,
} from "./commands.js";
export { EventStream } from "./event-stream.js";

export type {
  HostEvent,
  HostEventListener,
} from "./events.js";

export type {
  Api,
  AssistantMessage,
  ImageContent,
  KnownProvider,
  Message,
  Model,
  TextContent,
  ThinkingContent,
  ToolCall,
  ToolResultMessage,
  Usage,
  UserMessage,
} from "./messages.js";

export type {
  ModelCapabilities,
  ModelProviderConfig,
  ModelRunSettings,
  ModelRuntimeCounters,
  ModelRuntimeLimits,
  ToolInfo,
} from "./model.js";

export type {
  Orchestrator,
  OrchModelConfig,
  OrchRunCommandOptions,
  OrchRunOptions,
  OrchRunResult,
} from "./runtime.js";

export type { OrchState } from "./state.js";

export type {
  OrchestratorControlRef,
  ProviderNamespaceRef,
  ProviderToolRef,
  ToolApprovalPolicy,
  ToolApprovalRequirement,
  ToolCapability,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolExecutionMode,
  ToolExecutorRef,
  ToolExposure,
  ToolFailureMode,
  ToolMetadata,
  ToolPolicy,
  ToolProvider,
  ToolProviderSource,
  ToolSensitivity,
  ToolSet,
  ToolSetEntry,
  ToolSetMetadata,
  ToolSetPolicy,
  ToolSetToolRef,
} from "./tools.js";
