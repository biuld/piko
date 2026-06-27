// ---- piko-orch-protocol — public API ----
// Stable, serializable, Host-visible protocol types.

export type {
  AgentArtifact,
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
export { isApprovalAccepted } from "./approval.js";
export type {
  OrchestratorCommand,
  OrchestratorResponse,
} from "./commands.js";
export type {
  DebugSpan,
  DebugTraceInput,
  DebugTraceLevel,
  DebugTraceOutcome,
  DebugTraceRecord,
  DebugTraceSink,
} from "./debug-trace.js";
export {
  debugTrace,
  isDebugTraceEnabled,
  setDebugTraceSink,
  startDebugSpan,
} from "./debug-trace.js";
export { EventStream } from "./event-stream.js";

export type {
  OrchWireEvent,
  OrchWireEventListener,
} from "./events.js";

// === Unified HostEvent (new design — replaces old event layers) ===
export type {
  ApprovalDecision,
  domainEventTypes,
  HostEvent,
  HostEventApprovalRequested,
  HostEventApprovalResolved,
  HostEventAssistantMessageCompleted,
  HostEventMessageEnd,
  HostEventMessageStart,
  HostEventModelConfigChanged,
  HostEventQueueUpdate,
  HostEventSessionCreated,
  HostEventTaskCancelled,
  HostEventTaskCompleted,
  HostEventTaskCreated,
  HostEventTaskFailed,
  HostEventTaskJoined,
  HostEventTaskStarted,
  HostEventTaskSteered,
  HostEventTaskTranscriptCommitted,
  HostEventTextDelta,
  HostEventThinkingDelta,
  HostEventToolEnd,
  HostEventToolResultCommitted,
  HostEventToolStart,
  HostEventTurnCancelled,
  HostEventTurnCompleted,
  HostEventTurnFailed,
  HostEventTurnStarted,
  HostEventUserMessageSubmitted,
  isDomainEvent,
  isStreamingEvent,
  MessageRole,
  streamingEventTypes,
  ToolCallRef,
  Usage as HostEventUsage,
} from "./host-event.js";

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
  ModelCatalogEntry,
  ModelProviderConfig,
  ModelRunSettings,
  ModelRuntimeCounters,
  ModelRuntimeLimits,
  ModelSummary,
  ProviderInfo,
  ResolvedModel,
  ToolInfo,
} from "./model.js";
export type {
  HostEventParams,
  HostRpcMethods,
  HostToolCancelParams,
  HostToolExecuteParams,
  JsonRpcFailure,
  JsonRpcId,
  JsonRpcMessage,
  JsonRpcNotification,
  JsonRpcRequest,
  JsonRpcSuccess,
  OrchRpcMethods,
  OrchRpcNotifications,
  RegisterToolProviderParams,
} from "./rpc.js";
export type {
  Orchestrator,
  OrchestratorRuntimeConfig,
  OrchModelConfig,
  OrchRunCommandOptions,
  OrchRunOptions,
  OrchRunResult,
} from "./runtime.js";

export type {
  RuntimeAssistantContentBlock,
  RuntimeAssistantMessage,
  RuntimeAssistantMessageEvent,
  RuntimeCustomMessage,
  RuntimeMessage,
  RuntimeMessageRole,
  RuntimeOrder,
  RuntimeTextBlock,
  RuntimeThinkingBlock,
  RuntimeToolCallBlock,
  RuntimeToolOrder,
  RuntimeToolResultMessage,
  RuntimeUserContentBlock,
  RuntimeUserMessage,
} from "./runtime-stream.js";
export {
  providerPartialToRuntimeAssistant,
  runtimeToolEntityId,
  toMessage,
  toRuntimeMessage,
} from "./runtime-stream.js";

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
