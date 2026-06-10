import type { AgentOrchestrator } from "piko-agent-orchestrator";
import type {
  AgentSpec,
  EngineApprovalResolution,
  EngineEvent,
  EngineStepResult,
  EngineToolSet,
  OrchestratorEventEnvelope,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream } from "piko-engine-protocol";

// ============================================================================
// FauxEngine
// ============================================================================

export interface FauxStep {
  events?: EngineEvent[];
  result: EngineStepResult;
  gate?: { resolve: () => void; promise: Promise<void> };
}

export function makeFauxEngine(
  steps: FauxStep[],
  resolveApproval?: (r: EngineApprovalResolution) => Promise<EngineStepResult>,
): StatelessEngine {
  let idx = 0;
  return {
    capabilities: {
      supportsApprovals: true,
      supportsTools: true,
      supportsSandbox: false,
      supportsMCP: false,
      maxSteps: 100,
      tools: [],
    },
    executeStep(_input, _signal) {
      const stream = new EventStream<EngineEvent, EngineStepResult>();
      const step = steps[idx] ?? {
        result: { status: "error", appendedMessages: [], stopReason: "error" },
      };
      idx++;

      const emitAndEnd = () => {
        if (step.events) for (const e of step.events) stream.push(e);
        stream.end(step.result);
      };

      if (step.gate) {
        step.gate.promise.then(emitAndEnd);
      } else {
        Promise.resolve().then(emitAndEnd);
      }

      return stream;
    },
    resolveApproval,
  };
}

export function gate(): { resolve: () => void; promise: Promise<void> } {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { resolve, promise };
}

// ---- Step builders ----

export function assistantStep(
  text: string,
  status: EngineStepResult["status"] = "completed",
): FauxStep {
  return {
    events: [
      { type: "step_start" },
      { type: "message_delta", messageId: "m1", delta: text },
      { type: "message_end", message: { role: "assistant", content: [{ type: "text", text }] } },
    ],
    result: {
      status,
      appendedMessages: [{ role: "assistant", content: [{ type: "text", text }] }],
      stopReason: status === "completed" ? "assistant" : "tool",
    },
  };
}

export function approvalStep(toolName = "shell"): FauxStep {
  return {
    events: [
      { type: "step_start" },
      { type: "tool_call_start", id: "tc1", name: toolName, args: { command: "test" } },
    ],
    result: {
      status: "awaiting_resource",
      appendedMessages: [
        {
          role: "assistant",
          content: [
            { type: "toolCall", id: "tc1", name: toolName, arguments: { command: "test" } },
          ],
        },
      ],
      stopReason: "resource",
      pendingTools: {
        requestId: "tc1",
        kind: `tool:${toolName}`,
        details: { toolName, toolCallId: "tc1", arguments: { command: "test" } },
        engineState: {
          version: 1,
          kind: "pending_tools",
          pendingToolCalls: {
            assistantMessage: {} as never,
            remainingToolCallIds: ["tc1"],
            toolCalls: [],
            settings: {},
          },
        },
      },
    },
  };
}

// ============================================================================
// ToolSets & Agents
// ============================================================================

export const codingToolSet: EngineToolSet = {
  id: "builtin:core-coding",
  name: "Core Coding",
  tools: [
    {
      name: "shell",
      description: "Shell",
      inputSchema: {},
      executor: { kind: "native", target: "shell" },
      exposure: "direct",
      capabilities: ["execute_process"],
      approval: "always",
    },
  ],
};

export const readOnlyToolSet: EngineToolSet = {
  id: "builtin:read-only-shell",
  name: "Read-Only",
  tools: [
    {
      name: "shell",
      description: "RO",
      inputSchema: {},
      executor: { kind: "native", target: "shell" },
      exposure: "direct",
      capabilities: ["read_workspace"],
      approval: "never",
    },
  ],
};

export function makeAgent(
  id: string,
  role: string,
  toolSetIds: string[],
  extra?: Partial<AgentSpec>,
): AgentSpec {
  return { id, name: id, role, systemPrompt: `You are ${id}.`, toolSetIds, ...extra };
}

export function implementer(id = "implementer"): AgentSpec {
  return makeAgent(id, "Makes changes.", ["builtin:core-coding"], {
    concurrency: { requiresWriteLock: true, maxConcurrentTasks: 1 },
  });
}

export function reviewer(id = "reviewer"): AgentSpec {
  return makeAgent(id, "Reviews code.", ["builtin:read-only-shell"], {
    concurrency: { canRunInParallel: true },
  });
}

export function parallelAgent(id: string): AgentSpec {
  return makeAgent(id, id, ["builtin:core-coding"], { concurrency: { canRunInParallel: true } });
}

// ============================================================================
// Helpers
// ============================================================================

export function collect(orch: AgentOrchestrator) {
  const events: OrchestratorEventEnvelope[] = [];
  const unsub = orch.subscribe((env) => events.push(env));
  return { events, unsub };
}
