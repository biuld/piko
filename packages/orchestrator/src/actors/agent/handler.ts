// ---- AgentActor — handler factory ----

import type { AgentSpec } from "piko-orchestrator-protocol";
import type { ActorContext, ActorHandler } from "../../kernel/actor-system.js";
import type { Envelope } from "../../kernel/envelope.js";
import { startAgentRun } from "./runner.js";
import type { AgentActorDeps, AgentMsg, AgentRuntimeState } from "./types.js";

class AgentActorInstance {
  private readonly state: AgentRuntimeState;
  private readonly deps: AgentActorDeps;

  constructor(spec: AgentSpec, deps: AgentActorDeps) {
    this.state = {
      spec,
      status: "idle",
      nextRunToken: 1,
    };
    this.deps = deps;
  }

  async handle(msg: AgentMsg, ctx: ActorContext, meta: Envelope<AgentMsg>): Promise<void> {
    switch (msg.type) {
      case "dispatch":
        await this.handleDispatch(msg.task, ctx, meta);
        break;
      case "runner_finished":
        await this.handleRunnerFinished(msg.taskId, msg.token, msg.result, ctx);
        break;
      case "runner_failed":
        await this.handleRunnerFailed(msg.taskId, msg.token, msg.error, ctx);
        break;
      case "cancel":
        await this.handleCancel(msg.taskId, msg.reason, ctx, meta);
        break;
      case "wake":
        ctx.reply(meta, undefined);
        break;
      case "set_model_config":
        this.handleSetModelConfig(msg.config, ctx, meta);
        break;
    }
  }

  private async finalize(
    outcome: {
      type: "completed" | "failed" | "cancelled";
      result?: any;
      error?: string;
      reason?: string;
    },
    ctx: ActorContext,
  ): Promise<void> {
    if (this.state.terminalCommitted) return;
    this.state.terminalCommitted = true;

    const taskId = this.state.currentTaskId ?? "unknown";
    this.state.currentRunToken = undefined;
    this.state.status = "idle";
    this.state.currentTaskId = undefined;
    this.state.abortController = undefined;

    if (outcome.type === "completed" && outcome.result) {
      await this.deps.emit({
        type: "task_transcript_committed",
        agentId: this.state.spec.id,
        taskId,
        turnIndex: 0,
        messages: outcome.result.messages ?? [],
        summary: outcome.result.summary ?? "",
        finalStatus: outcome.result.finalStatus ?? "completed",
      });

      if (outcome.result.finalStatus === "aborted") {
        await this.deps.emit({
          type: "task_cancelled",
          agentId: this.state.spec.id,
          taskId,
          turnIndex: 0,
          reason: outcome.result.summary ?? "Task cancelled",
        });
      } else if (outcome.result.finalStatus === "error") {
        await this.deps.emit({
          type: "task_failed",
          agentId: this.state.spec.id,
          taskId,
          turnIndex: 0,
          error: outcome.result.summary ?? "An error occurred",
        });
      } else {
        await this.deps.emit({
          type: "task_completed",
          agentId: this.state.spec.id,
          taskId,
          turnIndex: 0,
          result: outcome.result,
        });
      }
    } else if (outcome.type === "failed") {
      await this.deps.emit({
        type: "task_failed",
        agentId: this.state.spec.id,
        taskId,
        turnIndex: 0,
        error: outcome.error ?? "Unknown error",
      });
    } else if (outcome.type === "cancelled") {
      await this.deps.emit({
        type: "task_cancelled",
        agentId: this.state.spec.id,
        taskId,
        turnIndex: 0,
        reason: outcome.reason ?? "Task cancelled",
      });
    }

    const pendingReply = this.state.pendingReply;
    this.state.pendingReply = undefined;
    if (pendingReply) {
      if (outcome.type === "completed" && outcome.result) {
        ctx.reply(pendingReply, outcome.result);
      } else if (outcome.type === "failed") {
        ctx.reply(pendingReply, {
          summary: outcome.error ?? "Error",
          messages: [],
          totalSteps: 0,
          finalStatus: "error",
        });
      } else {
        ctx.reply(pendingReply, {
          summary: outcome.reason ?? "Cancelled",
          messages: [],
          totalSteps: 0,
          finalStatus: "aborted",
        });
      }
    }

    await ctx.stop(ctx.self.id);
  }

  private async handleDispatch(
    task: import("piko-orchestrator-protocol").AgentTask,
    ctx: ActorContext,
    meta: Envelope<AgentMsg>,
  ): Promise<void> {
    if (this.state.status === "running") {
      ctx.reject(meta, new Error("Agent already running a task"));
      return;
    }

    this.state.status = "running";
    this.state.currentTaskId = task.id;
    this.state.pendingReply = meta;
    const runToken = this.state.nextRunToken++;
    this.state.currentRunToken = runToken;
    this.state.abortController = new AbortController();
    this.state.terminalCommitted = false;

    await this.deps.emit({
      type: "task_started",
      agentId: this.state.spec.id,
      taskId: task.id ?? "unknown",
      turnIndex: 0,
    });

    startAgentRun(this.state, this.deps, ctx, task, runToken);
  }

  private async handleRunnerFinished(
    taskId: string,
    token: number,
    result: any,
    ctx: ActorContext,
  ): Promise<void> {
    if (this.state.currentRunToken !== token || this.state.currentTaskId !== taskId) {
      return;
    }
    await this.finalize({ type: "completed", result }, ctx);
  }

  private async handleRunnerFailed(
    taskId: string,
    token: number,
    error: string,
    ctx: ActorContext,
  ): Promise<void> {
    if (this.state.currentRunToken !== token || this.state.currentTaskId !== taskId) {
      return;
    }
    if (this.state.status === "cancelling") {
      await this.finalize({ type: "cancelled", reason: error }, ctx);
    } else {
      await this.finalize({ type: "failed", error }, ctx);
    }
  }

  private async handleCancel(
    taskId: string,
    reason: string | undefined,
    ctx: ActorContext,
    meta: Envelope<AgentMsg>,
  ): Promise<void> {
    if (this.state.currentTaskId === taskId) {
      if (this.state.status === "running") {
        this.state.status = "cancelling";
        this.state.abortController?.abort(reason);
      }
    } else {
      ctx.reject(meta, new Error(`Task "${taskId}" is not owned by actor ${ctx.self.id}`));
      return;
    }
    ctx.reply(meta, undefined);
  }

  private handleSetModelConfig(config: any, ctx: ActorContext, meta: Envelope<AgentMsg>): void {
    if (config) {
      if (!this.deps.modelConfig) {
        this.deps.modelConfig = {
          model: {
            id: "default",
            name: "Default",
          } as import("piko-orchestrator-protocol").Model<string>,
          provider: {},
          settings: { allowToolCalls: true },
        };
      }
      if (config.model) {
        this.deps.modelConfig.model = {
          ...this.deps.modelConfig.model,
          ...config.model,
        } as import("piko-orchestrator-protocol").Model<string>;
      }
      if (config.provider) {
        this.deps.modelConfig.provider = {
          ...this.deps.modelConfig.provider,
          ...config.provider,
        };
      }
      if (config.settings) {
        this.deps.modelConfig.settings = {
          ...this.deps.modelConfig.settings,
          ...config.settings,
        };
      }
    }
    ctx.reply(meta, undefined);
  }
}

/** Create an AgentActor handler for the given spec and dependencies. */
export function agentActor(spec: AgentSpec, deps: AgentActorDeps): ActorHandler<AgentMsg> {
  const actor = new AgentActorInstance(spec, deps);
  return (msg, ctx, meta) => actor.handle(msg, ctx, meta);
}
