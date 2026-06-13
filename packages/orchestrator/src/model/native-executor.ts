// ---- Native ModelStepExecutor — in-process pi-ai based executor ----

import type { ModelCapabilities, ToolDef } from "piko-orchestrator-protocol";
import { createReadyContinuationState } from "./continuation-state.js";
import { EventStream } from "./event-stream.js";
import { runModelStepStateMachine } from "./step-state-machine.js";
import { executePendingToolCalls } from "./tool-runner.js";
import type {
  ModelResourceResolution,
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "./types.js";

export interface CreateNativeModelExecutorOptions {
  /** Additional or overriding tool definitions used only for validation (not execution). */
  toolDefinitions?: ToolDef[];
}

export function createNativeModelExecutor(
  options: CreateNativeModelExecutorOptions = {},
): ModelStepExecutor {
  const engineTools = options.toolDefinitions ?? [];

  const capabilities: ModelCapabilities = {
    supportsTools: engineTools.length > 0,
    supportsSandbox: false,
    supportsMCP: false,
    tools: engineTools.map((t) => ({ name: t.name, description: t.description })),
  };

  return {
    capabilities,

    executeStep(
      input: ModelStepInput,
      signal?: AbortSignal,
    ): EventStream<ModelStepEvent, ModelStepResult> {
      const stream = new EventStream<ModelStepEvent, ModelStepResult>();

      void runModelStepStateMachine(
        { ...input, tools: input.tools ?? [] },
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
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return stream;
    },

    async resolveResource(
      resolution: ModelResourceResolution,
      signal?: AbortSignal,
    ): Promise<ModelStepResult> {
      const raw = resolution.engineState;
      const continuationState =
        raw &&
        typeof raw === "object" &&
        "version" in raw &&
        (raw as { version: number }).version === 1
          ? (raw as import("./types.js").ModelContinuationState)
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
      const resumeContext = continuationState.resumeContext;

      // Continue with next provider call
      const input: ModelStepInput = {
        runId: resolution.runId,
        stepId: `${resolution.stepId}-resume`,
        transcript: updatedTranscript,
        systemPrompt: resumeContext.systemPrompt,
        model: resumeContext.model,
        provider: resumeContext.provider,
        tools: resumeContext.tools,
        settings: {
          ...resumeContext.settings,
          ...pending.settings,
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

      const stream = new EventStream<ModelStepEvent, ModelStepResult>();
      void runModelStepStateMachine(
        input,
        (event) => {
          if (signal?.aborted) return;
          stream.push(event);
        },
        signal,
      )
        .then((result) => stream.end(result))
        .catch((err) => {
          stream.push({
            type: "error",
            message: err instanceof Error ? err.message : String(err),
          });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return stream.result();
    },

    async shutdown(): Promise<void> {},
  };
}
