// ---- AgentActor — handler factory ----

import type { AgentSpec } from "piko-orchestrator-protocol";
import type { ActorContext, ActorHandler } from "../../kernel/actor-system.js";
import type { Envelope } from "../../kernel/envelope.js";
import { taskRunnerActor } from "./runner.js";
import type { AgentActorDeps, AgentMsg, AgentRuntimeState } from "./types.js";

/** Create an AgentActor handler for the given spec and dependencies. */
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
        state.pendingReply = meta;

        await deps.emit({
          type: "task_started",
          agentId: spec.id,
          taskId: task.id ?? "unknown",
        });

        const runnerId = `runner:${spec.id}:${task.id}`;
        ctx.spawn({
          id: runnerId,
          kind: "task_runner",
          handler: taskRunnerActor(ctx.self.id, state, deps, task) as ActorHandler,
        });
        state.currentRunnerId = runnerId;

        ctx.send(runnerId, { type: "run" });
        return;
      }

      case "runner_finished": {
        if (state.currentRunnerId) {
          await ctx.stop(state.currentRunnerId);
          state.currentRunnerId = undefined;
        }
        state.status = "idle";
        state.currentTaskId = undefined;
        const pendingReply = state.pendingReply;
        state.pendingReply = undefined;
        if (pendingReply) {
          ctx.reply(pendingReply, msg.result);
        }
        return;
      }

      case "runner_failed": {
        if (state.currentRunnerId) {
          await ctx.stop(state.currentRunnerId);
          state.currentRunnerId = undefined;
        }
        await deps.emit({
          type: "task_failed",
          agentId: spec.id,
          taskId: state.currentTaskId ?? "unknown",
          error: msg.error,
        });
        state.status = "idle";
        state.currentTaskId = undefined;
        const pendingReply = state.pendingReply;
        state.pendingReply = undefined;
        if (pendingReply) {
          ctx.reply(pendingReply, {
            summary: msg.error,
            messages: [],
            totalSteps: state.stepCount,
            finalStatus: "error",
          });
        }
        return;
      }

      case "cancel": {
        if (state.currentTaskId === msg.taskId) {
          if (state.currentRunnerId) {
            await ctx.stop(state.currentRunnerId);
            state.currentRunnerId = undefined;
          }
          state.status = "idle";
          state.currentTaskId = undefined;
          const pendingReply = state.pendingReply;
          state.pendingReply = undefined;
          if (pendingReply) {
            ctx.reply(pendingReply, {
              summary: "Task cancelled",
              messages: state.transcript,
              totalSteps: state.stepCount,
              finalStatus: "aborted",
            });
          }
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
