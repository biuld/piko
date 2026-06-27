import type { AgentSpec, AgentTask, AgentTaskId } from "./agents.js";
import type { ToolApprovalDecision, ToolApprovalRequest } from "./approval.js";
import type { OrchWireEvent } from "./events.js";
import type { ToolCall } from "./messages.js";
import type { OrchModelConfig, OrchRunResult } from "./runtime.js";
import type { OrchState } from "./state.js";
import type {
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProviderSource,
  ToolSet,
} from "./tools.js";

export type JsonRpcId = string | number;

export interface JsonRpcRequest<TParams = unknown> {
  jsonrpc: "2.0";
  id: JsonRpcId;
  method: string;
  params?: TParams;
}

export interface JsonRpcNotification<TParams = unknown> {
  jsonrpc: "2.0";
  method: string;
  params?: TParams;
}

export interface JsonRpcSuccess<TResult = unknown> {
  jsonrpc: "2.0";
  id: JsonRpcId;
  result: TResult;
}

export interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: JsonRpcId | null;
  error: {
    code: number;
    message: string;
    data?: unknown;
  };
}

export type JsonRpcMessage = JsonRpcRequest | JsonRpcNotification | JsonRpcSuccess | JsonRpcFailure;

export interface RegisterToolProviderParams {
  providerId: string;
  source: ToolProviderSource;
}

export interface HostToolExecuteParams {
  providerId: string;
  executionId: string;
  call: ToolCall;
  context: ToolExecutionContext;
}

export interface HostToolCancelParams {
  executionId: string;
  reason?: string;
}

export interface HostEventParams {
  event: OrchWireEvent;
}

export interface OrchRpcMethods {
  "orch.configure": {
    params: unknown;
    result: null;
  };
  "orch.set_model_config": {
    params: OrchModelConfig;
    result: null;
  };
  "orch.register_agent": {
    params: AgentSpec;
    result: null;
  };
  "orch.unregister_agent": {
    params: { agentId: string };
    result: null;
  };
  "orch.register_tool_provider": {
    params: RegisterToolProviderParams;
    result: null;
  };
  "orch.unregister_tool_provider": {
    params: { providerId: string };
    result: null;
  };
  "orch.register_tool_set": {
    params: ToolSet;
    result: null;
  };
  "orch.unregister_tool_set": {
    params: { toolSetId: string };
    result: null;
  };
  "orch.start_task": {
    params: AgentTask;
    result: { taskId: AgentTaskId };
  };
  "orch.await_task": {
    params: { taskId: AgentTaskId };
    result: OrchRunResult;
  };
  "orch.cancel_task": {
    params: { taskId: AgentTaskId; reason?: string };
    result: null;
  };
  "orch.snapshot": {
    params: Record<string, never>;
    result: OrchState;
  };
  "orch.get_graph": {
    params: Record<string, never>;
    result: {
      nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
      edges: Array<{ from: string; to: string; label?: string }>;
    };
  };
  "orch.update_plan": {
    params: { agentId: string; taskId: string; plan: unknown[] };
    result: null;
  };
  "orch.subscribe_events": {
    params: Record<string, never>;
    result: { subscribed: true };
  };
}

export interface HostRpcMethods {
  "host.tools.discover": {
    params: { providerId: string; context: ToolDiscoveryContext };
    result: ToolDef[];
  };
  "host.tools.execute": {
    params: HostToolExecuteParams;
    result: ToolExecResult;
  };
  "host.approval.request": {
    params: { request: ToolApprovalRequest };
    result: ToolApprovalDecision;
  };
}

export interface OrchRpcNotifications {
  "host.tools.cancel": HostToolCancelParams;
  host_event: HostEventParams;
}
