// ---- StateActor — event log, reducer, subscriptions, snapshots ----

import type {
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskResult,
  AgentTaskState,
  HostEventListener,
  OrchState,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { ActorHandler } from "../kernel/actor-system.js";

export type OrchestratorEvent =
  | { type: "orchestrator_started" }
  | { type: "orchestrator_stopped"; reason?: string }
  | { type: "actor_spawned"; actorId: string; kind: string }
  | { type: "actor_stopped"; actorId: string; reason?: string }
  | { type: "actor_error"; actorId: string; message: string }
  | { type: "agent_registered"; agent: AgentSpec }
  | { type: "agent_unregistered"; agentId: string }
  | { type: "task_created"; task: AgentTask }
  | { type: "task_started"; agentId: string; taskId: string }
  | { type: "task_delta"; agentId: string; taskId: string; delta: unknown }
  | {
      type: "task_completed";
      agentId: string;
      taskId: string;
      result: AgentTaskResult;
    }
  | {
      type: "task_transcript_committed";
      agentId: string;
      taskId: string;
      messages: import("piko-orchestrator-protocol").Message[];
      summary: string;
      finalStatus: string;
    }
  | { type: "task_failed"; agentId: string; taskId: string; error: string }
  | {
      type: "task_cancelled";
      agentId: string;
      taskId: string;
      reason?: string;
    }
  | {
      type: "plan_updated";
      agentId: string;
      taskId: string;
      plan: unknown;
    }
  | {
      type: "tool_started";
      agentId: string;
      taskId: string;
      callId: string;
      name: string;
      args: Record<string, unknown>;
    }
  | {
      type: "tool_finished";
      agentId: string;
      taskId: string;
      callId: string;
      result: unknown;
    }
  | {
      type: "approval_requested";
      approval: unknown;
    }
  | {
      type: "approval_resolved";
      approvalId: string;
      decision: string;
    };

export interface OrchestratorEventEnvelope {
  id: string;
  runId: string;
  seq: number;
  time: number;
  event: OrchestratorEvent;
}

// ---- State messages ----

type StateMsg =
  | { type: "ingest_event"; event: OrchestratorEvent }
  | { type: "snapshot" }
  | { type: "dump_events" }
  | { type: "render_graph" }
  | { type: "subscribe"; listener: HostEventListener }
  | { type: "unsubscribe"; subscriptionId: string };

// ---- State ----

export interface StateActorState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  eventLog: OrchestratorEventEnvelope[];
  seq: number;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  locks: Record<string, unknown>;
  listeners: Map<string, HostEventListener>;
  nextSubId: number;
  /** Tool call metadata for HostEvent mapping. */
  callMetas: Map<string, CallMeta>;
  /** ToolSet registry (populated by facade, readable by snapshot). */
  toolSets: Record<string, ToolSet>;
}

function _createInitialState(runId: string): StateActorState {
  return {
    runId,
    status: "idle",
    eventLog: [],
    seq: 0,
    agents: {},
    tasks: {},
    locks: {},
    listeners: new Map(),
    nextSubId: 1,
    callMetas: new Map(),
    toolSets: {},
  };
}

// ---- Call metadata (for HostEvent mapping) ----

interface CallMeta {
  name: string;
  args: Record<string, unknown>;
}

// ---- Pure reducer ----

function reduce(state: StateActorState, env: OrchestratorEventEnvelope): void {
  const event = env.event;

  switch (event.type) {
    case "orchestrator_started":
      state.status = "running";
      break;
    case "orchestrator_stopped":
      state.status = "stopped";
      break;
    case "agent_registered": {
      const existing = state.agents[event.agent.id];
      state.agents[event.agent.id] = {
        id: event.agent.id,
        spec: event.agent,
        status: "idle",
        transcript: existing?.transcript ?? [],
      };
      break;
    }
    case "agent_unregistered":
      delete state.agents[event.agentId];
      break;
    case "task_created": {
      const task = event.task;
      const taskId = task.id ?? `task_${env.seq}`;
      state.tasks[taskId] = {
        id: taskId,
        targetAgentId: task.targetAgentId,
        prompt: task.prompt,
        source: task.source,
        status: "queued",
        priority: task.priority ?? 0,
        parentTaskId: task.parentTaskId,
      };
      break;
    }
    case "task_started": {
      const task = state.tasks[event.taskId];
      if (task) task.status = "running";
      // Update agent state
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "running";
        agent.activeTaskId = event.taskId;
      }
      break;
    }
    case "task_completed": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "completed";
        task.result = event.result;
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "task_transcript_committed": {
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.transcript = event.messages;
      }
      break;
    }
    case "task_failed": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "failed";
        task.error = event.error;
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "task_cancelled": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "cancelled";
        task.error = event.reason ?? "Cancelled";
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "plan_updated": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.result = {
          ...task.result,
          summary: task.result?.summary ?? "",
          artifacts: task.result?.artifacts ?? [],
          plan: event.plan,
        } as import("piko-orchestrator-protocol").AgentTaskResult & { plan: unknown };
      }
      break;
    }
    case "tool_started": {
      state.callMetas.set(event.callId, {
        name: event.name,
        args: event.args ?? {},
      });
      break;
    }
    case "tool_finished":
      // Call meta already tracked; no additional state mutation needed
      break;
    case "approval_requested":
    case "approval_resolved":
      // Approval state lives in HostToolProvider; no reducer state needed
      break;
    case "actor_spawned":
    case "actor_stopped":
    case "actor_error":
      // Infrastructure events; recorded in event log only
      break;
  }
}

// ---- StateActor handler ----

export function stateActor(state: StateActorState): ActorHandler<StateMsg> {
  return async (msg, ctx, meta) => {
    switch (msg.type) {
      case "ingest_event": {
        state.seq++;
        const envelope: OrchestratorEventEnvelope = {
          id: `evt_${state.seq}`,
          runId: state.runId,
          seq: state.seq,
          time: Date.now(),
          event: msg.event,
        };
        state.eventLog.push(envelope);
        reduce(state, envelope);

        // Notify listeners
        const hostEvent = eventToHostEvent(msg.event, envelope, state);
        for (const listener of state.listeners.values()) {
          try {
            if (hostEvent) listener(hostEvent);
          } catch {
            // ignore listener errors
          }
        }

        ctx.reply(meta, envelope);
        return;
      }

      case "snapshot": {
        ctx.reply(meta, structuredClone(buildSnapshot(state)));
        return;
      }

      case "dump_events": {
        ctx.reply(meta, structuredClone(state.eventLog));
        return;
      }

      case "render_graph": {
        ctx.reply(meta, buildGraph(state));
        return;
      }

      case "subscribe": {
        const id = `sub_${state.nextSubId++}`;
        state.listeners.set(id, msg.listener);
        ctx.reply(meta, { id, unsubscribe: () => state.listeners.delete(id) });
        return;
      }

      case "unsubscribe": {
        state.listeners.delete(msg.subscriptionId);
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

// ---- Helpers ----

function buildSnapshot(state: StateActorState): OrchState {
  return {
    runId: state.runId,
    status: state.status,
    toolSets: state.toolSets,
    agents: state.agents,
    tasks: state.tasks,
  };
}

function eventToHostEvent(
  event: OrchestratorEvent,
  _env: OrchestratorEventEnvelope,
  state: StateActorState,
): import("piko-orchestrator-protocol").HostEvent | null {
  switch (event.type) {
    case "orchestrator_started":
      return null;
    case "orchestrator_stopped":
      return { type: "done", status: "stopped" };
    case "agent_registered":
      return null;
    case "agent_unregistered":
      return null;
    case "task_created":
      return {
        type: "task_created",
        task: {
          ...event.task,
          id: event.task.id ?? "",
          targetAgentId: event.task.targetAgentId,
        },
      };
    case "task_started":
      return {
        type: "task_started",
        taskId: event.taskId,
        agentId: event.agentId,
      };
    case "task_delta": {
      // task_delta could be a token, thinking, or other content
      const delta = event.delta as { kind?: string; text?: string };
      if (delta?.kind === "thinking" && delta.text) {
        return {
          type: "thinking",
          agentId: event.agentId,
          taskId: event.taskId,
          text: delta.text,
        };
      }
      if (delta?.text) {
        return {
          type: "token",
          agentId: event.agentId,
          taskId: event.taskId,
          text: delta.text,
        };
      }
      return null;
    }
    case "task_completed":
      return {
        type: "task_completed",
        taskId: event.taskId,
        agentId: event.agentId,
        result: event.result,
      };
    case "task_transcript_committed":
      return {
        type: "task_transcript_committed",
        taskId: event.taskId,
        agentId: event.agentId,
        messages: event.messages,
        summary: event.summary,
        finalStatus: event.finalStatus,
      };
    case "task_failed":
      return {
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.error,
      };
    case "task_cancelled":
      return {
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.reason ?? "Cancelled",
      };
    case "tool_started": {
      const meta = state.callMetas.get(event.callId);
      return {
        type: "tool_start",
        agentId: event.agentId,
        taskId: event.taskId,
        id: event.callId,
        name: event.name,
        args: meta?.args ?? {},
      };
    }
    case "tool_finished": {
      const meta = state.callMetas.get(event.callId);
      const result = event.result as { ok?: boolean; error?: unknown } | undefined;
      return {
        type: "tool_end",
        agentId: event.agentId,
        taskId: event.taskId,
        id: event.callId,
        name: meta?.name ?? "",
        result: result,
        isError: result && typeof result === "object" && "ok" in result ? !result.ok : false,
      };
    }
    case "approval_requested":
      return {
        type: "approval_needed",
        approvalId: (event.approval as { id?: string })?.id ?? "",
        agentId: "",
        taskId: "",
        toolName: "",
        toolArgs: {},
      };
    case "approval_resolved":
      return {
        type: "approval_resolved",
        approvalId: event.approvalId,
        agentId: "",
        taskId: "",
        decision: event.decision as "accept" | "decline",
      };
    default:
      return null;
  }
}

// ---- Graph projection ----

interface GraphNode {
  id: string;
  label: string;
  kind: string;
  status?: string;
}

interface GraphEdge {
  from: string;
  to: string;
  label?: string;
}

function buildGraph(state: StateActorState): {
  nodes: GraphNode[];
  edges: GraphEdge[];
} {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  for (const [id, agent] of Object.entries(state.agents)) {
    nodes.push({
      id: `agent:${id}`,
      label: agent.spec.name,
      kind: "agent",
      status: agent.status,
    });
    if (agent.activeTaskId) {
      edges.push({
        from: `agent:${id}`,
        to: `task:${agent.activeTaskId}`,
        label: "owns",
      });
    }
  }

  for (const [id, task] of Object.entries(state.tasks)) {
    nodes.push({
      id: `task:${id}`,
      label: task.prompt.slice(0, 50),
      kind: "task",
      status: task.status,
    });
    if (task.parentTaskId) {
      edges.push({
        from: `task:${task.parentTaskId}`,
        to: `task:${id}`,
        label: "parent",
      });
    }
  }

  return { nodes, edges };
}

// ---- Factory ----

export function createStateActor(state: StateActorState) {
  return {
    handler: stateActor(state),
    state,
  };
}
