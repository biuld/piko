import { EventStream } from "@earendil-works/pi-ai";
import type {
  StatelessEngine,
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineApprovalResolution,
  EngineCapabilities,
} from "piko-engine-protocol";
import type { NativeToolRegistry } from "./types.ts";
import { runStepStateMachine, runApprovalResolution } from "./state-machine.js";

export interface CreateNativeEngineOptions {
  cwd?: string;
  tools?: NativeToolRegistry;
}

export function createNativeEngine(
  options?: CreateNativeEngineOptions,
): StatelessEngine {
  const cwd = options?.cwd ?? process.cwd();
  const toolRegistry: NativeToolRegistry = options?.tools ?? {};

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
      const stream = new EventStream<EngineEvent, EngineStepResult>(
        () => false,
        () => {
          throw new Error("Result should be set via end()");
        },
      );

      // Run the state machine asynchronously
      runStepStateMachine(input, toolRegistry, (event) => {
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
      const stream = new EventStream<EngineEvent, EngineStepResult>(
        () => false,
        () => {
          throw new Error("Result should be set via end()");
        },
      );

      // Run approval resolution asynchronously, but wait for result
      const resultPromise = stream.result();

      runApprovalResolution(request, toolRegistry, (event) => {
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
