import type {
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskResult,
  AgentTaskState,
  HostEventListener,
  RuntimeAssistantMessageEvent,
  RuntimeMessage,
  ToolSet,
} from "piko-orchestrator-protocol";

// ---- Ordering metadata shared across lifecycle events -------
interface OrchOrderBase {
  /** Strictly increasing within runId (filled by caller, propagated by StateActor). */
  eventSeq?: number;

  /** Zero-based model step within the run. */
  turnIndex?: number;

  /** Stable logical position of this message within the run. */
  messageIndex?: number;
}

export interface OrchToolOrder {
  /** Assistant message containing the corresponding toolCall block. */
  parentMessageId?: string;

  /** Position of the toolCall block in parent assistant content. */
  contentIndex?: number;

  /** Dense position among tool calls in that assistant message. */
  toolCallIndex?: number;
}

export type OrchestratorEvent =
  | { type: "orchestrator_started" }
  | { type: "orchestrator_stopped"; reason?: string }
  | { type: "actor_spawned"; actorId: string; kind: string }
  | { type: "actor_stopped"; actorId: string; reason?: string }
  | { type: "actor_error"; actorId: string; message: string }
  | { type: "agent_registered"; agent: AgentSpec }
  | { type: "agent_unregistered"; agentId: string }
  | { type: "tool_set_registered"; toolSet: ToolSet }
  | { type: "tool_set_unregistered"; toolSetId: string }
  | { type: "task_created"; task: AgentTask }
  | (OrchOrderBase & { type: "task_started"; agentId: string; taskId: string })
  | (OrchOrderBase & {
      type: "task_message_start";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
    })
  | (OrchOrderBase & {
      type: "task_message_update";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
      assistantEvent?: RuntimeAssistantMessageEvent;
    })
  | (OrchOrderBase & {
      type: "task_message_end";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
    })
  | { type: "task_delta"; agentId: string; taskId: string; delta: unknown }
  | (OrchOrderBase & {
      type: "task_completed";
      agentId: string;
      taskId: string;
      result: AgentTaskResult;
    })
  | (OrchOrderBase & {
      type: "task_transcript_committed";
      agentId: string;
      taskId: string;
      messages: import("piko-orchestrator-protocol").Message[];
      summary: string;
      finalStatus: string;
    })
  | (OrchOrderBase & {
      type: "task_failed";
      agentId: string;
      taskId: string;
      error: string;
    })
  | (OrchOrderBase & {
      type: "task_cancelled";
      agentId: string;
      taskId: string;
      reason?: string;
    })
  | (OrchOrderBase & {
      type: "plan_updated";
      agentId: string;
      taskId: string;
      plan: unknown;
    })
  | (OrchOrderBase &
      OrchToolOrder & {
        type: "tool_started";
        agentId: string;
        taskId: string;
        callId: string;
        name: string;
        args: Record<string, unknown>;
      })
  | (OrchOrderBase &
      OrchToolOrder & {
        type: "tool_finished";
        agentId: string;
        taskId: string;
        callId: string;
        result: unknown;
      })
  | (OrchOrderBase & {
      type: "approval_requested";
      approval: unknown;
    })
  | (OrchOrderBase & {
      type: "approval_resolved";
      approvalId: string;
      decision: string;
    });

export interface OrchestratorEventEnvelope {
  id: string;
  runId: string;
  seq: number;
  time: number;
  event: OrchestratorEvent;
}

export type StateMsg =
  | { type: "ingest_event"; event: OrchestratorEvent }
  | { type: "snapshot" }
  | { type: "dump_events" }
  | { type: "render_graph" }
  | { type: "subscribe"; listener: HostEventListener }
  | { type: "unsubscribe"; subscriptionId: string };

export interface CallMeta {
  name: string;
  args: Record<string, unknown>;
}

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
  /** ToolSet registry projected from tool_set_* events. */
  toolSets: Record<string, ToolSet>;
}
