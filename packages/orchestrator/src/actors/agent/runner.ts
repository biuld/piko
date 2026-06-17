// ---- AgentActor — async run handle ----
// This keeps long model/tool loops off the AgentActor mailbox without pretending
// the runner is an isolated actor.

import type { AgentTask } from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import { runEngineLoop } from "./loop.js";
import type { AgentActorDeps, AgentRuntimeState } from "./types.js";

export function startAgentRun(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
  token: number,
): void {
  void runEngineLoop(state, deps, ctx, task)
    .then((result) => {
      ctx.send(ctx.self.id, {
        type: "runner_finished",
        taskId: task.id ?? "unknown",
        token,
        result,
      });
    })
    .catch((err) => {
      const errorMsg = err instanceof Error ? err.message : String(err);
      ctx.send(ctx.self.id, {
        type: "runner_failed",
        taskId: task.id ?? "unknown",
        token,
        error: errorMsg,
      });
    });
}
