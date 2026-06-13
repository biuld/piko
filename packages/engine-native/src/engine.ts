import type {
  EngineCapabilities,
  EngineEvent,
  EngineInput,
  EngineResourceResolution,
  EngineStepResult,
  EventStream,
  StatelessEngine,
  ToolDef,
} from "piko-protocol";
import { EventStream as EventStreamImpl } from "piko-protocol";
import { createReadyContinuationState } from "./state/continuation-state.js";
import { runStepStateMachine } from "./state/index.js";
import { createBuiltinCodingToolSet } from "./tools/index.js";
import { executePendingToolCalls } from "./tools/runner.js";
import type { NativeToolRegistry } from "./types.js";

function mergeToolDefinitions(
  builtin: ToolDef[],
  extraRegistry: NativeToolRegistry,
  extraDefs?: ToolDef[],
): ToolDef[] {
  const extraNames = new Set(Object.keys(extraRegistry));
  const defByName = new Map<string, ToolDef>();
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
  toolDefinitions?: ToolDef[];
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
    supportsTools: engineTools.length > 0,
    supportsSandbox: false,
    supportsMCP: false,
    tools: engineTools.map((t) => ({ name: t.name, description: t.description })),
  };

  return {
    capabilities,

    executeStep(
      input: EngineInput,
      signal?: AbortSignal,
    ): EventStream<EngineEvent, EngineStepResult> {
      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();

      void runStepStateMachine(
        { ...input, tools: input.tools ?? [] },
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

    async resolveResource(
      resolution: EngineResourceResolution,
      signal?: AbortSignal,
    ): Promise<EngineStepResult> {
      const raw = resolution.engineState;
      const continuationState =
        raw &&
        typeof raw === "object" &&
        "version" in raw &&
        (raw as { version: number }).version === 1
          ? (raw as import("piko-protocol").EngineContinuationState)
          : undefined;
      if (continuationState?.kind !== "pending_tools") {
        return { status: "error", appendedMessages: [], stopReason: "error" };
      }

      const pending = continuationState.pendingToolCalls;

      // Apply tool results to transcript
      const toolMessages = resolution.toolResults
        ? executePendingToolCalls(
            pending.toolCalls.map((tc) => ({
              id: tc.id,
              name: tc.name,
              arguments: tc.args,
              executorTarget: tc.executorTarget,
              executionMode: tc.executionMode,
            })),
            resolution.toolResults,
          )
        : [];

      const updatedTranscript = [...resolution.transcript, ...toolMessages];

      // Continue with next provider call
      const input: EngineInput = {
        runId: resolution.runId,
        stepId: `${resolution.stepId}-resume`,
        transcript: updatedTranscript,
        systemPrompt: "",
        model: {} as never,
        provider: {},
        tools: [],
        settings: {
          maxSteps: pending.settings?.maxSteps ?? 10,
          allowToolCalls: pending.settings?.allowToolCalls ?? true,
          parallelTools: pending.settings?.parallelTools,
          runtimeLimits: pending.settings?.runtimeLimits,
        },
        engineState: createReadyContinuationState(
          continuationState.counters ?? {
            modelCalls: 0,
            toolCalls: 0,
            consecutiveErrors: 0,
            startedAt: Date.now(),
          },
        ),
      };

      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();
      void runStepStateMachine(
        input,
        toolRegistry,
        (event) => {
          if (signal?.aborted) return;
          stream.push(event);
        },
        signal,
      )
        .then((result) => stream.end(result))
        .catch((err) => {
          stream.push({ type: "error", message: err instanceof Error ? err.message : String(err) });
          stream.end({ status: "error", appendedMessages: [], stopReason: "error" });
        });

      return stream.result();
    },

    async shutdown(): Promise<void> {},
  };
}
