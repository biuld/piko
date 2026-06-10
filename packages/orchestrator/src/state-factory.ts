import type { OrchestratorState } from "piko-orchestrator-protocol";

let orchestratorCounter = 0;

export function createOrchestratorState(runId?: string): OrchestratorState {
  return {
    runId: runId ?? `orch-${Date.now()}-${orchestratorCounter++}`,
    status: "idle",
    toolSets: {},
    agents: {},
    tasks: {},
    watches: {},
    locks: {},
    approvals: {},
    artifacts: {},
  };
}
