import type {
  EngineApprovalResolution,
  EngineCapabilities,
  EngineEvent,
  EngineInput,
  EngineStepResult,
  EngineTool,
  EventStream,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import { runApprovalResolution, runStepStateMachine } from "./state-machine.js";
import { createBuiltinCodingToolSet } from "./tools/index.js";
import type { NativeToolRegistry } from "./types.js";

function mergeToolDefinitions(
  builtin: EngineTool[],
  extraRegistry: NativeToolRegistry,
  extraDefs?: EngineTool[],
): EngineTool[] {
  const extraNames = new Set(Object.keys(extraRegistry));
  const defByName = new Map<string, EngineTool>();
  if (extraDefs) {
    for (const def of extraDefs) defByName.set(def.name, def);
  }
  const merged = builtin.filter((t) => !extraNames.has(t.name));
  for (const name of extraNames) {
    const existing = defByName.get(name);
    merged.push(
      existing ?? {
        name,
        description: `Custom tool: ${name}`,
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native", target: name },
      },
    );
  }
  return merged;
}

export interface CreateNativeEngineOptions {
  cwd?: string;
  /** Additional or overriding tools. When absent, engine uses built-in coding tools. */
  toolRegistry?: NativeToolRegistry;
  /** Tool definitions for custom tools (only needed with toolRegistry). */
  toolDefinitions?: EngineTool[];
}

export function createNativeEngine(options: CreateNativeEngineOptions = {}): StatelessEngine {
  const cwd = options.cwd ?? process.cwd();

  const builtin = createBuiltinCodingToolSet(cwd);
  const toolRegistry: NativeToolRegistry = options.toolRegistry
    ? { ...builtin.registry, ...options.toolRegistry }
    : builtin.registry;
  const engineTools = options.toolDefinitions
    ? mergeToolDefinitions(builtin.definitions, options.toolRegistry ?? {}, options.toolDefinitions)
    : builtin.definitions;

  const capabilities: EngineCapabilities = {
    supportsApprovals: true,
    supportsTools: engineTools.length > 0,
    supportsSandbox: false,
    supportsMCP: false,
    maxSteps: 100,
    tools: engineTools.map((t) => ({ name: t.name, description: t.description })),
    engineTools,
  };

  return {
    capabilities,

    executeStep(
      input: EngineInput,
      signal?: AbortSignal,
    ): EventStream<EngineEvent, EngineStepResult> {
      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();

      void runStepStateMachine(
        // Use caller-provided tools when explicitly set.
        // undefined/missing → use engineTools (backward compat).
        // [] → explicitly no tools.
        { ...input, tools: input.tools ?? engineTools },
        toolRegistry,
        (event) => {
          if (signal?.aborted) return;
          stream.push(event);
        },
        signal,
      )
        .then((result) => stream.end(result))
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({ status: "error", appendedMessages: [], stopReason: "error" });
        });

      return stream;
    },

    async resolveApproval(
      request: EngineApprovalResolution,
      signal?: AbortSignal,
    ): Promise<EngineStepResult> {
      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();
      const resultPromise = stream.result();

      void runApprovalResolution(
        request,
        toolRegistry,
        (event) => {
          if (signal?.aborted) return;
          stream.push(event);
        },
        signal,
      )
        .then((result) => stream.end(result))
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({ status: "error", appendedMessages: [], stopReason: "error" });
        });

      return resultPromise;
    },

    async shutdown(): Promise<void> {},
  };
}
