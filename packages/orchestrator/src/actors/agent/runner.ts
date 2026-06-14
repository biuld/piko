// ---- AgentActor — TaskRunnerActor (Worker) ----
// This actor runs the actual runEngineLoop asynchronously.

import type { AgentTask } from "piko-orchestrator-protocol";
import type { ActorContext, ActorHandler } from "../../kernel/actor-system.js";
import type { Envelope } from "../../kernel/envelope.js";
import { runEngineLoop } from "./loop.js";
import type { AgentActorDeps, AgentRuntimeState } from "./types.js";

export type TaskRunnerMsg = { type: "run" };

export function taskRunnerActor(
  supervisorId: string,
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  task: AgentTask,
): ActorHandler<TaskRunnerMsg> {
  return async (msg: TaskRunnerMsg, ctx: ActorContext, meta: Envelope<TaskRunnerMsg>) => {
    switch (msg.type) {
      case "run": {
        try {
          const result = await runEngineLoop(state, deps, ctx, task);
          ctx.send(supervisorId, { type: "runner_finished", result });
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          ctx.send(supervisorId, { type: "runner_failed", error: errorMsg });
        }
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}
