// ---- AgentActor — model loop, transcript, task state ----

import type {
  AgentSpec,
  AgentTask,
  AgentTaskResult,
  EventStream,
  Message,
  ModelProviderConfig,
  ModelRunSettings,
  ToolDef,
  ToolExecResult,
} from "piko-orchestrator-protocol";
import type { ActorContext, ActorHandler } from "../kernel/actor-system.js";
import type { Envelope } from "../kernel/envelope.js";
import type {
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "../model/types.js";
import type { ToolRegistry } from "../tool-registry.js";
import type { OrchestratorEvent } from "./state.js";

// ---- Messages ----

export type AgentMsg =
  | { type: "dispatch"; task: AgentTask }
  | { type: "cancel"; taskId: string; reason?: string }
  | {
      type: "wake";
      reason: { type: string; taskId?: string; approvalId?: string };
    }
  | {
      type: "set_model_config";
      config: {
        model?: { id: string; name?: string; provider?: string };
        provider?: Record<string, unknown>;
        settings?: { maxSteps?: number; allowToolCalls?: boolean; allowApprovals?: boolean };
      };
    };

// ---- Agent private state ----

interface AgentRuntimeState {
  spec: AgentSpec;
  status: "idle" | "running" | "failed" | "stopped";
  currentTaskId?: string;
  transcript: Message[];
  engineState?: unknown;
  stepCount: number;
  cancelled: Set<string>;
}

// ---- Dependencies ----

export interface AgentActorDeps {
  modelExecutor: ModelStepExecutor;
  emit: (event: OrchestratorEvent) => Promise<void>;
  maxSteps?: number;
  modelConfig?: {
    model: import("piko-orchestrator-protocol").Model<string>;
    provider: ModelProviderConfig;
    settings: ModelRunSettings;
  };
  actorSystem?: import("../kernel/actor-system.js").ActorSystem;
  /** DI container for prototype ToolActor creation. */
  toolRegistry: ToolRegistry;
}

// ---- AgentActor handler factory ----

export function agentActor(spec: AgentSpec, deps: AgentActorDeps): ActorHandler<AgentMsg> {
  const state: AgentRuntimeState = {
    spec,
    status: "idle",
    transcript: [],
    stepCount: 0,
    cancelled: new Set(),
  };

  return async (msg: AgentMsg, ctx: ActorContext, meta: Envelope<AgentMsg>) => {
    switch (msg.type) {
      case "dispatch": {
        const task = msg.task;

        if (state.status === "running") {
          ctx.reject(meta, new Error("Agent already running a task"));
          return;
        }

        state.status = "running";
        state.currentTaskId = task.id;
        state.stepCount = 0;
        state.transcript = [
          {
            role: "user",
            content: task.prompt,
            timestamp: Date.now(),
          },
        ];

        await deps.emit({
          type: "task_started",
          agentId: spec.id,
          taskId: task.id ?? "unknown",
        });

        try {
          const result = await runEngineLoop(state, deps, ctx, task);
          ctx.reply(meta, result);
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          await deps.emit({
            type: "task_failed",
            agentId: spec.id,
            taskId: task.id ?? "unknown",
            error: errorMsg,
          });
          state.status = "idle";
          ctx.reply(meta, {
            summary: errorMsg,
            messages: [],
            totalSteps: state.stepCount,
            finalStatus: "error",
          });
        }
        return;
      }

      case "cancel": {
        if (state.currentTaskId === msg.taskId) {
          state.cancelled.add(msg.taskId);
        }
        await deps.emit({
          type: "task_cancelled",
          agentId: spec.id,
          taskId: msg.taskId,
          reason: msg.reason,
        });
        ctx.reply(meta, undefined);
        return;
      }

      case "wake": {
        ctx.reply(meta, undefined);
        return;
      }

      case "set_model_config": {
        if (msg.config) {
          if (!deps.modelConfig) {
            deps.modelConfig = {
              model: {
                id: "default",
                name: "Default",
              } as import("piko-orchestrator-protocol").Model<string>,
              provider: {},
              settings: { maxSteps: 50, allowToolCalls: true },
            };
          }
          if (msg.config.model) {
            deps.modelConfig.model = {
              ...deps.modelConfig.model,
              ...msg.config.model,
            } as import("piko-orchestrator-protocol").Model<string>;
          }
          if (msg.config.provider) {
            deps.modelConfig.provider = {
              ...deps.modelConfig.provider,
              ...msg.config.provider,
            };
          }
          if (msg.config.settings) {
            deps.modelConfig.settings = {
              ...deps.modelConfig.settings,
              ...msg.config.settings,
            };
          }
        }
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

// ---- Step-loop types ----

/** Terminal result from a step (cancelled / error / aborted / completed / max_steps). */
type StepTerminal = AgentTaskResult & {
  messages: Message[];
  totalSteps: number;
  finalStatus: string;
};

/** Outcome of a single step: either continue the loop, or return a terminal result. */
type StepOutcome = { kind: "continue" } | { kind: "terminal"; result: StepTerminal };

// ---- Model step loop ----

async function runEngineLoop(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
): Promise<AgentTaskResult & { messages: Message[]; totalSteps: number; finalStatus: string }> {
  const maxSteps = deps.maxSteps || state.spec.maxSteps || 50;
  const executor = deps.modelExecutor;
  const modelSettings = deps.modelConfig?.settings ?? {
    maxSteps: 1,
    allowToolCalls: true,
  };
  const model =
    deps.modelConfig?.model ??
    ({
      id: "default",
      name: "Default",
    } as import("piko-orchestrator-protocol").Model<string>);
  const provider = deps.modelConfig?.provider ?? {};
  const taskId = task.id ?? "unknown";

  while (state.stepCount < maxSteps) {
    const cancelled = checkCancelled(state, taskId);
    if (cancelled) return cancelled;

    state.stepCount++;

    const { toolId, tools } = await spawnAndDiscover(state, deps, ctx, taskId);
    try {
      const stepResult = await runModelStep(
        state,
        deps,
        executor,
        model,
        provider,
        modelSettings,
        tools,
        taskId,
      );

      const outcome = await processStepOutcome(
        state,
        deps,
        ctx,
        toolId,
        taskId,
        stepResult,
        modelSettings,
      );
      if (outcome.kind === "terminal") return outcome.result;
    } finally {
      await deps.toolRegistry.stopToolActor(toolId);
    }
  }

  return buildMaxStepsResult(state, deps, taskId, maxSteps);
}

// ---- Step helpers ----

function checkCancelled(state: AgentRuntimeState, taskId: string): StepTerminal | null {
  if (!state.cancelled.has(taskId)) return null;

  state.status = "idle";
  state.cancelled.delete(taskId);
  return {
    summary: "Task cancelled",
    messages: state.transcript,
    totalSteps: state.stepCount,
    finalStatus: "aborted",
  };
}

/** Spawn a fresh ToolActor and discover tools for this step. */
async function spawnAndDiscover(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
): Promise<{ toolId: string; tools: ToolDef[] }> {
  const toolId = deps.toolRegistry.spawnToolActor(`tool:${state.spec.id}:step_${state.stepCount}`);

  let tools: ToolDef[] = [];
  if (deps.actorSystem) {
    try {
      tools = await deps.actorSystem.ask<ToolDef[]>(
        toolId,
        {
          type: "discover_tools",
          context: {
            agentId: state.spec.id,
            taskId,
            toolSetIds: state.spec.toolSetIds,
            activeToolNames: state.spec.activeToolNames,
          },
        },
        ctx.self.id,
      );
    } catch {
      // Continue without tools if discovery fails
    }
  }

  return { toolId, tools };
}

/** Call the model executor with the current transcript, stream deltas via emit. */
async function runModelStep(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  executor: ModelStepExecutor,
  model: import("piko-orchestrator-protocol").Model<string>,
  provider: ModelProviderConfig,
  settings: ModelRunSettings,
  tools: ToolDef[],
  taskId: string,
): Promise<ModelStepResult> {
  const input: ModelStepInput = {
    runId: taskId,
    stepId: `step_${state.stepCount}`,
    transcript: state.transcript,
    systemPrompt: state.spec.systemPrompt,
    model,
    provider,
    settings,
    tools,
    engineState: state.engineState,
  };

  const stream: EventStream<ModelStepEvent, ModelStepResult> = executor.executeStep(input);

  for await (const event of stream) {
    switch (event.type) {
      case "message_delta":
        await deps.emit({
          type: "task_delta",
          agentId: state.spec.id,
          taskId,
          delta: { kind: "text", text: event.delta },
        });
        break;
      case "thinking_delta":
        await deps.emit({
          type: "task_delta",
          agentId: state.spec.id,
          taskId,
          delta: { kind: "thinking", text: event.delta },
        });
        break;
      case "message_end":
        state.transcript.push(event.message);
        break;
      case "step_start":
      case "step_end":
      case "error":
        break;
    }
  }

  const stepResult = await stream.result();

  // Merge appended messages into transcript
  for (const msg of stepResult.appendedMessages ?? []) {
    if (!state.transcript.includes(msg)) {
      state.transcript.push(msg);
    }
  }

  state.engineState = stepResult.engineState;
  return stepResult;
}

/** Process the step result: handle error/abort, extract tool calls, execute them. */
async function processStepOutcome(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  toolId: string,
  taskId: string,
  stepResult: ModelStepResult,
  modelSettings: ModelRunSettings,
): Promise<StepOutcome> {
  // ---- Terminal: error / aborted ----
  if (stepResult.status === "error" || stepResult.status === "aborted") {
    const summary = stepResult.status === "error" ? "Unknown engine error" : "Task aborted";
    return terminalStep(state, summary, stepResult.status);
  }

  // ---- Extract assistant message ----
  const assistantMessage = stepResult.appendedMessages?.find((m) => m.role === "assistant");
  if (!assistantMessage) {
    return { kind: "continue" };
  }

  const toolCalls = (
    Array.isArray(assistantMessage.content)
      ? assistantMessage.content.filter(
          (c: unknown) => (c as { type?: string }).type === "toolCall",
        )
      : []
  ) as Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>;

  // ---- No tool calls: task completed ----
  if (toolCalls.length === 0 || !modelSettings.allowToolCalls) {
    const text =
      typeof assistantMessage.content === "string"
        ? assistantMessage.content
        : JSON.stringify(assistantMessage.content);
    const summary = text.slice(0, 200);

    await deps.emit({
      type: "task_completed",
      agentId: state.spec.id,
      taskId,
      result: { summary },
    });

    return terminalStep(state, summary, "completed");
  }

  // ---- Execute tool calls ----
  await executeToolCalls(state, deps, ctx, toolId, taskId, toolCalls);
  return { kind: "continue" };
}

/** Execute a batch of tool calls via the step's ToolActor, appending results to transcript. */
async function executeToolCalls(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  toolId: string,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
): Promise<void> {
  if (!deps.actorSystem) return;

  for (const tc of toolCalls) {
    try {
      const execResult = await deps.actorSystem.ask<ToolExecResult>(
        toolId,
        {
          type: "execute",
          call: { id: tc.id, name: tc.name, arguments: tc.arguments },
          context: {
            agentId: state.spec.id,
            taskId,
            toolSetIds: state.spec.toolSetIds,
          },
        },
        ctx.self.id,
      );

      appendToolResult(state, tc, execResult);
    } catch (err) {
      const errorText = err instanceof Error ? err.message : String(err);
      state.transcript.push({
        role: "toolResult",
        toolName: tc.name,
        toolCallId: tc.id,
        content: [{ type: "text", text: `Tool error: ${errorText}` }],
        details: { error: errorText },
        isError: true,
        timestamp: Date.now(),
      } as Message);
    }
  }
}

/** Append a successful tool execution result to the transcript. */
function appendToolResult(
  state: AgentRuntimeState,
  tc: { id: string; name: string },
  execResult: ToolExecResult,
): void {
  const text =
    typeof execResult.value === "string"
      ? execResult.value
      : JSON.stringify(execResult.ok ? execResult.value : execResult.error, null, 2);

  state.transcript.push({
    role: "toolResult",
    toolName: tc.name,
    toolCallId: tc.id,
    content: [{ type: "text", text }],
    details: execResult.ok ? execResult.value : execResult.error,
    isError: !execResult.ok,
    timestamp: Date.now(),
  } as Message);
}

/** Mark agent idle and return a terminal outcome. */
function terminalStep(state: AgentRuntimeState, summary: string, finalStatus: string): StepOutcome {
  state.status = "idle";
  state.currentTaskId = undefined;
  return {
    kind: "terminal",
    result: {
      summary,
      messages: state.transcript,
      totalSteps: state.stepCount,
      finalStatus,
    },
  };
}

/** Build the max-steps-reached terminal result. */
function buildMaxStepsResult(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  taskId: string,
  maxSteps: number,
): StepTerminal {
  state.status = "idle";
  state.currentTaskId = undefined;

  const finalMsg =
    state.transcript
      .filter((m: Message) => m.role === "assistant")
      .map((m: Message) => (typeof m.content === "string" ? m.content : JSON.stringify(m.content)))
      .join("\n") || "Max steps reached";

  const error = `Max steps (${maxSteps}) reached.`;
  deps.emit({ type: "task_failed", agentId: state.spec.id, taskId, error }).catch(() => {});

  return {
    summary: `${error} ${finalMsg}`,
    messages: state.transcript,
    totalSteps: state.stepCount,
    finalStatus: "max_steps",
  };
}
