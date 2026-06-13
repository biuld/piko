// ---- MainActor — top-level run/task coordination ----

import type { AgentSpec, AgentTask, AgentTaskId, OrchRunResult } from "piko-orchestrator-protocol";
import type { ActorHandler, ActorSystem } from "../kernel/actor-system.js";
import type { Message } from "../model/event-stream.js";
import { type AgentActorDeps, agentActor } from "./agent.js";
import type { OrchestratorEvent } from "./state.js";

// ---- Messages ----

type MainMsg =
  | { type: "register_agent"; spec: AgentSpec }
  | { type: "unregister_agent"; agentId: string }
  | { type: "dispatch"; task: AgentTask }
  | {
      type: "run";
      prompt: string;
      options?: {
        targetAgentId?: string;
        signal?: AbortSignal;
        maxSteps?: number;
      };
    }
  | { type: "cancel_task"; taskId: string; reason?: string }
  | { type: "set_model_config"; config: unknown };

// ---- Private state ----

interface MainActorState {
  agents: Map<string, AgentSpec>;
  taskOwners: Map<AgentTaskId, string>;
  defaultAgentId?: string;
  /** Active run signals for cancellation. */
  activeRuns: Map<string, { signal?: AbortSignal; onAbort?: () => void }>;
}

// ---- MainActor handler factory ----

export function mainActor(
  state: MainActorState,
  deps: {
    actorSystem: ActorSystem;
    stateActorId: string;
    emit: (event: OrchestratorEvent) => Promise<void>;
    createAgentDeps: () => AgentActorDeps;
  },
): ActorHandler<MainMsg> {
  return async (msg, ctx, meta) => {
    switch (msg.type) {
      case "register_agent": {
        const spec = msg.spec;
        state.agents.set(spec.id, spec);

        // Spawn agent actor
        const handler = agentActor(spec, deps.createAgentDeps());
        deps.actorSystem.spawn({
          id: `agent:${spec.id}`,
          kind: "agent",
          handler: handler as ActorHandler,
        });

        await deps.emit({ type: "agent_registered", agent: spec });
        ctx.reply(meta, undefined);
        return;
      }

      case "unregister_agent": {
        state.agents.delete(msg.agentId);
        for (const [taskId, ownerId] of state.taskOwners) {
          if (ownerId === msg.agentId) {
            state.taskOwners.delete(taskId);
          }
        }
        await deps.actorSystem.stop(`agent:${msg.agentId}`);
        await deps.emit({
          type: "agent_unregistered",
          agentId: msg.agentId,
        });
        ctx.reply(meta, undefined);
        return;
      }

      case "dispatch": {
        const task = msg.task;
        const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
        const normalizedTask: AgentTask = {
          ...task,
          id: taskId,
          targetAgentId: task.targetAgentId || state.defaultAgentId || "main",
        };

        state.taskOwners.set(taskId, normalizedTask.targetAgentId);
        await deps.emit({ type: "task_created", task: normalizedTask });

        // Dispatch to agent actor
        const agentResult = await deps.actorSystem.ask<{ taskId: string }>(
          `agent:${normalizedTask.targetAgentId}`,
          { type: "dispatch", task: normalizedTask },
          ctx.self.id,
        );

        ctx.reply(meta, agentResult);
        return;
      }

      case "run": {
        const { prompt, options } = msg;
        const targetAgentId = options?.targetAgentId || state.defaultAgentId || "main";
        const signal = options?.signal;

        if (!state.agents.has(targetAgentId)) {
          throw new Error(`Agent "${targetAgentId}" not registered.`);
        }

        await deps.emit({ type: "orchestrator_started" });

        const task: AgentTask = {
          targetAgentId,
          prompt,
          source: { type: "user" },
        };
        const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
        task.id = taskId;

        state.taskOwners.set(taskId, targetAgentId);
        await deps.emit({ type: "task_created", task });

        // Wire cancellation
        const onAbort = () => {
          try {
            deps.actorSystem.send(`agent:${targetAgentId}`, {
              type: "cancel",
              taskId,
              reason: "Aborted by signal",
            });
          } catch {
            // Actor may not exist yet
          }
        };

        if (signal) {
          if (signal.aborted) {
            ctx.reply(meta, buildRunResult([], 0, "aborted"));
            return;
          }
          signal.addEventListener("abort", onAbort, { once: true });
        }

        try {
          const agentResult = await deps.actorSystem.ask<{
            messages: Message[];
            totalSteps: number;
            finalStatus: string;
          }>(`agent:${targetAgentId}`, { type: "dispatch", task }, ctx.self.id);

          ctx.reply(
            meta,
            buildRunResult(
              agentResult.messages ?? [],
              agentResult.totalSteps ?? 1,
              mapStatus(agentResult.finalStatus),
            ),
          );
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          await deps.emit({
            type: "task_failed",
            agentId: targetAgentId,
            taskId,
            error: errorMsg,
          });
          ctx.reply(meta, buildRunResult([], 0, "error"));
        } finally {
          if (signal) {
            signal.removeEventListener("abort", onAbort);
          }
        }
        return;
      }

      case "cancel_task": {
        const ownerId = state.taskOwners.get(msg.taskId);
        if (!ownerId) {
          throw new Error(`Task "${msg.taskId}" not found`);
        }

        await deps.actorSystem.ask(
          `agent:${ownerId}`,
          { type: "cancel", taskId: msg.taskId, reason: msg.reason },
          ctx.self.id,
        );
        ctx.reply(meta, undefined);
        return;
      }

      case "set_model_config": {
        const config = msg.config as {
          model?: { id: string; name?: string; provider?: string };
          provider?: Record<string, unknown>;
          settings?: { maxSteps?: number; allowToolCalls?: boolean; allowApprovals?: boolean };
        };
        // Forward to all registered agent actors
        for (const [agentId] of state.agents) {
          try {
            deps.actorSystem.send(`agent:${agentId}`, {
              type: "set_model_config",
              config,
            });
          } catch {
            // Agent may not be spawned yet
          }
        }
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

function buildRunResult(
  messages: Message[],
  totalSteps: number,
  status: "completed" | "aborted" | "error" | "max_steps",
): OrchRunResult {
  return { messages, totalSteps, status };
}

function mapStatus(s: string): "completed" | "aborted" | "error" | "max_steps" {
  switch (s) {
    case "completed":
      return "completed";
    case "aborted":
      return "aborted";
    case "error":
      return "error";
    case "max_steps":
      return "max_steps";
    default:
      return "completed";
  }
}

// ---- Factory ----

export function createMainActor(deps: {
  actorSystem: ActorSystem;
  stateActorId: string;
  emit: (event: OrchestratorEvent) => Promise<void>;
  createAgentDeps: () => AgentActorDeps;
}) {
  const state: MainActorState = {
    agents: new Map(),
    taskOwners: new Map(),
    activeRuns: new Map(),
  };

  return {
    handler: mainActor(state, deps),
    state,
  };
}
