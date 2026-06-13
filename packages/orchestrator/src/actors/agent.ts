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
  toolActorId?: string;
  modelConfig?: {
    model: import("piko-orchestrator-protocol").Model<string>;
    provider: ModelProviderConfig;
    settings: ModelRunSettings;
  };
  actorSystem?: import("../kernel/actor-system.js").ActorSystem;
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

    // ---- Discover tools ----
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
              activeToolNames: state.spec.activeToolNames,
            },
          },
          ctx.self.id,
        );
      } catch {
        // Continue without tools if discovery fails
      }
    }

    const input: ModelStepInput = {
      runId: taskId,
      stepId: `step_${state.stepCount}`,
      transcript: state.transcript,
      systemPrompt: state.spec.systemPrompt,
      model,
      provider,
      settings: modelSettings,
      tools,
      engineState: state.engineState,
    };

    // ---- Call model executor ----
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

    for (const msg of stepResult.appendedMessages ?? []) {
      if (!state.transcript.includes(msg)) {
        state.transcript.push(msg);
      }
    }

    state.engineState = stepResult.engineState;

    if (stepResult.status === "error") {
      const errMsg = "Unknown engine error";
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

    if (stepResult.status === "aborted") {
      state.status = "idle";
      state.currentTaskId = undefined;
      await notifyToolTaskFinished(deps, state.spec.id, taskId);
      return {
        summary: "Task aborted",
        messages: state.transcript,
        totalSteps: state.stepCount,
        finalStatus: "aborted",
      };
    }

    // ---- Check assistant message for tool calls ----
    const assistantMessage = stepResult.appendedMessages?.find((m) => m.role === "assistant");
    if (!assistantMessage) {
      // No assistant message — task is done or errored
      continue;
    }

    const toolCalls = Array.isArray(assistantMessage.content)
      ? assistantMessage.content.filter(
          (c: unknown) => (c as { type?: string }).type === "toolCall",
        )
      : [];

    if (toolCalls.length === 0 || !modelSettings.allowToolCalls) {
      // Assistant finished without tool calls — task completed
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

    // ---- Execute tool calls via ToolActor ----
    if (!deps.actorSystem || !deps.toolActorId) {
      // No tool actor available — can't execute tools
      continue;
    }

    for (const tc of toolCalls as Array<{
      id: string;
      name: string;
      arguments: Record<string, unknown>;
    }>) {
      try {
        const execResult = await deps.actorSystem.ask<ToolExecResult>(
          deps.toolActorId,
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

    // Tool results are in transcript — continue loop to call LLM again
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
