import type { AgentTask } from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import { runEngineLoop } from "./loop.js";
import type { AgentActorDeps, AgentRuntimeState, AgentWorkerState } from "./types.js";

export function startAgentRun(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
  token: number,
): void {
  const initialTranscript = [
    ...(task.history ?? []),
    {
      role: "user",
      content: task.prompt,
      timestamp: Date.now(),
    },
  ];

  const workerState: AgentWorkerState = {
    transcript: initialTranscript as any[],
    stepCount: 0,
  };

  const signal = state.abortController?.signal;

  void runEngineLoop(state, workerState, deps, ctx, task, signal)
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
