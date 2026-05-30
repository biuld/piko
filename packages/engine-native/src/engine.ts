import type {
  StatelessEngine,
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineApprovalResolution,
  EngineCapabilities,
  EventStream,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { NativeToolRegistry } from "./types.js";
import type { LlmCaller } from "./llm-caller.js";
import { runStepStateMachine, runApprovalResolution } from "./state-machine.js";

export interface CreateNativeEngineOptions {
  llmCaller: LlmCaller;
  cwd?: string;
  tools?: NativeToolRegistry;
}

export function createNativeEngine(
  options: CreateNativeEngineOptions,
): StatelessEngine {
  const llmCaller = options.llmCaller;
  const toolRegistry: NativeToolRegistry = options.tools ?? {};

  const capabilities: EngineCapabilities = {
    supportsApprovals: true,
    supportsTools: true,
    supportsSandbox: false,
    supportsMCP: false,
    maxSteps: 100,
  };

  return {
    capabilities,

    executeStep(
      input: EngineInput,
      signal?: AbortSignal,
    ): EventStream<EngineEvent, EngineStepResult> {
      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();

      void runStepStateMachine(input, llmCaller, toolRegistry, (event) => {
        if (signal?.aborted) return;
        stream.push(event);
      }, signal)
        .then((result) => {
          stream.end(result);
        })
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return stream;
    },

    async resolveApproval(
      request: EngineApprovalResolution,
      signal?: AbortSignal,
    ): Promise<EngineStepResult> {
      const stream = new EventStreamImpl<EngineEvent, EngineStepResult>();
      const resultPromise = stream.result();

      void runApprovalResolution(request, toolRegistry, (event) => {
        if (signal?.aborted) return;
        stream.push(event);
      }, signal)
        .then((result) => {
          stream.end(result);
        })
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return resultPromise;
    },

    async shutdown(): Promise<void> {
      // No-op for native engine
    },
  };
}
