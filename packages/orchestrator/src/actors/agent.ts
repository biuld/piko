// ---- AgentActor — engine loop, transcript, task state ----

import type {
  EngineEvent,
  EngineInput,
  EngineStepResult,
  EventStream,
  Message,
  StatelessEngine,
  ToolDef,
  ToolExecResult,
} from "piko-protocol";
import type { ActorContext, ActorHandler } from "../kernel/actor-system.js";
import type { Envelope } from "../kernel/envelope.js";
import type { AgentSpec, AgentTask, AgentTaskResult } from "../types.js";
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
      type: "set_engine_config";
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
  engine: StatelessEngine;
  emit: (event: OrchestratorEvent) => Promise<void>;
  maxSteps?: number;
  toolActorId?: string;
  engineConfig?: {
    model: import("piko-protocol").Model<string>;
    provider: import("piko-protocol").EngineProviderConfig;
    settings: import("piko-protocol").EngineRunSettings;
  };
  actorSystem?: import("../kernel/actor-system.js").ActorSystem;
}

// ---- AgentActor handler factory ----

export function agentActor(spec: AgentSpec, deps: AgentActorDeps): ActorHandler<AgentMsg> {
  const state: AgentRuntimeState = {
    spec,
    status: "idle",
    transcript: [], // Reset per task; starts fresh on each dispatch
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

      case "set_engine_config": {
        // Initialize or update engine config
        if (msg.config) {
          if (!deps.engineConfig) {
            deps.engineConfig = {
              model: {
                id: "default",
                name: "Default",
              } as import("piko-protocol").Model<string>,
              provider: {},
              settings: { maxSteps: 50, allowToolCalls: true },
            };
          }
          if (msg.config.model) {
            deps.engineConfig.model = {
              ...deps.engineConfig.model,
              ...msg.config.model,
            } as import("piko-protocol").Model<string>;
          }
          if (msg.config.provider) {
            deps.engineConfig.provider = {
              ...deps.engineConfig.provider,
              ...msg.config.provider,
            };
          }
          if (msg.config.settings) {
            deps.engineConfig.settings = {
              ...deps.engineConfig.settings,
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

// ---- Engine loop ----

async function runEngineLoop(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
): Promise<AgentTaskResult & { messages: Message[]; totalSteps: number; finalStatus: string }> {
  const maxSteps = deps.maxSteps || state.spec.maxSteps || 50;
  const engine = deps.engine;
  const engineSettings = deps.engineConfig?.settings ?? {
    maxSteps: 1,
    allowToolCalls: true,
  };
  const model =
    deps.engineConfig?.model ??
    ({
      id: "default",
      name: "Default",
    } as import("piko-protocol").Model<string>);
  const provider = deps.engineConfig?.provider ?? {};
  const taskId = task.id ?? "unknown";

  while (state.stepCount < maxSteps) {
    // Check cancellation
    if (state.cancelled.has(taskId)) {
      state.status = "idle";
      state.cancelled.delete(taskId);
      return {
        summary: "Task cancelled",
        messages: state.transcript,
        totalSteps: state.stepCount,
        finalStatus: "aborted",
      };
    }

    state.stepCount++;

    // ---- Discover tools before engine call ----
    let tools: ToolDef[] = [];
    if (deps.actorSystem && deps.toolActorId) {
      try {
        tools = await deps.actorSystem.ask<ToolDef[]>(
          deps.toolActorId,
          {
            type: "discover_tools",
            context: {
              agentId: state.spec.id,
              taskId,
              toolSetIds: state.spec.toolSetIds,
            },
          },
          ctx.self.id,
        );
      } catch {
        // Continue without tools if discovery fails
      }
    }

    const input: EngineInput = {
      runId: taskId,
      stepId: `step_${state.stepCount}`,
      transcript: state.transcript,
      systemPrompt: state.spec.systemPrompt,
      model,
      provider,
      settings: engineSettings,
      tools, // <-- discovered tools
      engineState: state.engineState,
    };

    // ---- Call engine ----
    const stream: EventStream<EngineEvent, EngineStepResult> = engine.executeStep(input);

    // Stream deltas (only from engine; ToolActor emits its own lifecycle)
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
        case "resource_requested":
        case "error":
          break;
      }
    }

    const stepResult = await stream.result();

    // Append any missed messages
    for (const msg of stepResult.appendedMessages ?? []) {
      if (!state.transcript.includes(msg)) {
        state.transcript.push(msg);
      }
    }

    state.engineState = stepResult.engineState;

    switch (stepResult.status) {
      case "completed": {
        const summary =
          stepResult.appendedMessages
            ?.filter((m: Message) => m.role === "assistant")
            .map((m: Message) =>
              typeof m.content === "string"
                ? m.content.slice(0, 200)
                : JSON.stringify(m.content).slice(0, 200),
            )
            .join("\n") || "Task completed";

        await deps.emit({
          type: "task_completed",
          agentId: state.spec.id,
          taskId,
          result: { summary },
        });

        state.status = "idle";
        state.currentTaskId = undefined;
        await notifyToolTaskFinished(deps, state.spec.id, taskId);
        return {
          summary,
          messages: state.transcript,
          totalSteps: state.stepCount,
          finalStatus: "completed",
        };
      }

      case "awaiting_resource": {
        // Tool execution via ToolActor
        const stepWithTools = stepResult as {
          pendingTools?: import("piko-protocol").PendingToolCallState;
        };
        const pendingTools = stepWithTools.pendingTools;

        if (!pendingTools || pendingTools.remainingToolCallIds.length === 0) {
          continue;
        }

        const toolResults: Array<{
          toolCallId: string;
          result: unknown;
          isError: boolean;
        }> = [];

        for (const tc of pendingTools.toolCalls) {
          try {
            const execResult = await deps.actorSystem!.ask<ToolExecResult>(
              deps.toolActorId ?? "tool:registry",
              {
                type: "execute",
                call: { id: tc.id, name: tc.name, arguments: tc.args },
                context: {
                  agentId: state.spec.id,
                  taskId,
                  toolSetIds: state.spec.toolSetIds,
                },
              },
              ctx.self.id,
            );

            toolResults.push({
              toolCallId: tc.id,
              result: execResult.ok ? execResult.value : execResult.error,
              isError: !execResult.ok,
            });
          } catch (err) {
            toolResults.push({
              toolCallId: tc.id,
              result: {
                code: "tool_error",
                message: err instanceof Error ? err.message : String(err),
              },
              isError: true,
            });
          }
        }

        // Feed results back to engine
        if (engine.resolveResource) {
          const resolution = await engine.resolveResource({
            runId: taskId,
            stepId: `step_${state.stepCount}_resolve`,
            transcript: state.transcript,
            engineState: stepResult.engineState,
            toolResults,
          });

          for (const msg of resolution.appendedMessages ?? []) {
            if (!state.transcript.includes(msg)) {
              state.transcript.push(msg);
            }
          }
          state.engineState = resolution.engineState;
        }

        continue;
      }

      case "continue":
        continue;

      case "error": {
        const errObj = stepResult as { errorMessage?: string };
        const errMsg = errObj.errorMessage || "Unknown engine error";
        await deps.emit({
          type: "task_failed",
          agentId: state.spec.id,
          taskId,
          error: errMsg,
        });
        state.status = "idle";
        state.currentTaskId = undefined;
        await notifyToolTaskFinished(deps, state.spec.id, taskId);
        return {
          summary: errMsg,
          messages: state.transcript,
          totalSteps: state.stepCount,
          finalStatus: "error",
        };
      }

      case "aborted":
        state.status = "idle";
        state.currentTaskId = undefined;
        await notifyToolTaskFinished(deps, state.spec.id, taskId);
        return {
          summary: "Task aborted",
          messages: state.transcript,
          totalSteps: state.stepCount,
          finalStatus: "aborted",
        };

      default:
        break;
    }
  }

  // Max steps reached
  state.status = "idle";
  state.currentTaskId = undefined;

  const finalMsg =
    state.transcript
      .filter((m: Message) => m.role === "assistant")
      .map((m: Message) => (typeof m.content === "string" ? m.content : JSON.stringify(m.content)))
      .join("\n") || "Max steps reached";

  const result = {
    summary: `Max steps (${maxSteps}) reached. ${finalMsg}`,
    messages: state.transcript,
    totalSteps: state.stepCount,
    finalStatus: "max_steps",
  };

  await deps.emit({
    type: "task_failed",
    agentId: state.spec.id,
    taskId,
    error: `Max steps (${maxSteps}) reached.`,
  });
  await notifyToolTaskFinished(deps, state.spec.id, taskId);

  return result;
}

async function notifyToolTaskFinished(
  deps: AgentActorDeps,
  agentId: string,
  taskId: string,
): Promise<void> {
  if (!deps.actorSystem || !deps.toolActorId) return;
  try {
    await deps.actorSystem.ask(deps.toolActorId, {
      type: "task_finished",
      agentId,
      taskId,
    });
  } catch {
    // Best effort
  }
}
