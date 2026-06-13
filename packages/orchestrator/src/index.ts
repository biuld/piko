// ---- piko-orchestrator — public API ----

export {
  buildContinuationState,
  createReadyContinuationState,
  getOrCreateCounters,
} from "./model/continuation-state.js";
export type {
  Api,
  AssistantMessage,
  ImageContent,
  KnownProvider,
  Message,
  Model,
  TextContent,
  ThinkingContent,
  TokenUsage,
  ToolCall,
  ToolResultMessage,
  UserMessage,
} from "./model/event-stream.js";
export {
  EventStream,
  getEnvApiKey,
  getModel,
  getModels,
  getProviders,
} from "./model/event-stream.js";
// ---- Model step executor (internal subsystem, available for custom executors) ----
export { createNativeModelExecutor } from "./model/native-executor.js";
export { runModelStepStateMachine } from "./model/step-state-machine.js";
export { executePendingToolCalls, prepareToolCalls } from "./model/tool-runner.js";
// ---- Model step executor types (internal orchestrator subsystem) ----
export type {
  ModelCapabilities,
  ModelContinuationState,
  ModelEventEnvelope,
  ModelProviderConfig,
  ModelResourceResolution,
  ModelResumeContext,
  ModelRunSettings,
  ModelRuntimeCounters,
  ModelRuntimeLimits,
  ModelStepCompute,
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
  ModelStepStatus,
  PendingToolCallState,
  PendingToolsContinuationState,
  ReadyContinuationState,
  StopReason,
  ToolInfo,
  TranscriptDelta,
} from "./model/types.js";
export { Orchestrator } from "./orchestrator.js";
export { OrchestratorToolProvider } from "./providers/orchestrator-provider.js";
export type {
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
  ToolProviderSource,
} from "./tools/provider.js";

// ---- Tool protocol types ----
export type {
  OrchestratorControlRef,
  ProviderNamespaceRef,
  ProviderToolRef,
  ToolApprovalPolicy,
  ToolApprovalRequirement,
  ToolCapability,
  ToolDef,
  ToolExecutionMode,
  ToolExecutorRef,
  ToolExposure,
  ToolFailureMode,
  ToolMetadata,
  ToolPolicy,
  ToolSensitivity,
  ToolSet,
  ToolSetEntry,
  ToolSetMetadata,
  ToolSetPolicy,
  ToolSetToolRef,
} from "./tools/types.js";
// ---- Orchestrator-level types ----
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
  ApprovalGateway,
  HostEvent,
  HostEventListener,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  TaskSource,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "./types.js";
