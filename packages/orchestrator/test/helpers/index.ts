// ---- Shared test helpers for orchestrator tests ----

import type {
  AgentTaskResult,
  EventStream,
  Message,
  ModelCapabilities,
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
  ToolProviderSource,
} from "piko-orchestrator-protocol";
import { EventStream as ES } from "piko-orchestrator-protocol";
import type {
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "../../src/model/types.js";

// ---- Re-export EventStream with alias to avoid confusion ----
export { ES as TestEventStream };

// ---- Mock ToolProvider ----

export interface MockToolProviderOptions {
  id?: string;
  source?: ToolProviderSource;
  tools?: ToolDef[];
  executeResult?: ToolExecResult;
  executeFn?: (call: ToolCall, context: ToolExecutionContext) => Promise<ToolExecResult>;
  discoverFn?: (context: ToolDiscoveryContext) => Promise<ToolDef[]>;
}

export function createMockToolProvider(opts: MockToolProviderOptions = {}): ToolProvider {
  const tools = opts.tools ?? [];
  const defaultResult: ToolExecResult = opts.executeResult ?? { ok: true, value: "done" };

  return {
    id: opts.id ?? "mock-provider",
    source: opts.source ?? "workspace",
    discover: opts.discoverFn ?? (async () => tools),
    execute: opts.executeFn ?? (async () => defaultResult),
  };
}

// ---- Faux ModelStepExecutor ----

export interface FauxStepSpec {
  /** Assistant message content (string or content array). */
  content?: Message["content"];
  /** Tool calls to emit in the message. */
  toolCalls?: Array<{ id: string; name: string; arguments: Record<string, unknown> }>;
  /** Stream deltas before the step completes. */
  deltas?: Array<{ type: "text"; text: string } | { type: "thinking"; text: string }>;
  /** Final step result status. */
  status?: ModelStepResult["status"];
  /** Additional messages to append. */
  appendedMessages?: Message[];
  /** Stop reason. */
  stopReason?: string;
  /** Throw an error instead of returning a result. */
  throwError?: string;
  /** Delay before emitting deltas/completing (ms). */
  delayMs?: number;
}

export interface FauxModelExecutorOptions {
  steps?: FauxStepSpec[];
}

export function createFauxModelExecutor(opts: FauxModelExecutorOptions = {}): ModelStepExecutor {
  const steps = opts.steps ?? [];
  let stepIndex = 0;

  const capabilities: ModelCapabilities = {
    supportsTools: true,
    supportsSandbox: false,
    supportsMCP: false,
    tools: [],
  };

  return {
    capabilities,

    executeStep(
      _input: ModelStepInput,
      signal?: AbortSignal,
    ): EventStream<ModelStepEvent, ModelStepResult> {
      const stream = new ES<ModelStepEvent, ModelStepResult>();
      const spec = steps[stepIndex] ?? { content: "Done." };
      stepIndex++;

      void (async () => {
        // Check abort
        if (signal?.aborted) {
          stream.end({
            status: "aborted",
            appendedMessages: [],
            stopReason: "abort",
          });
          return;
        }

        // Apply delay if specified
        if (spec.delayMs) {
          await new Promise((r) => setTimeout(r, spec.delayMs));
        }

        // Simulate async
        await Promise.resolve();

        if (spec.throwError) {
          stream.push({ type: "error", message: spec.throwError });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
          return;
        }

        // Emit deltas
        for (const delta of spec.deltas ?? []) {
          if (signal?.aborted) break;
          if (delta.type === "text") {
            stream.push({ type: "message_delta", messageId: "assistant", delta: delta.text });
          } else {
            stream.push({ type: "thinking_delta", messageId: "assistant", delta: delta.text });
          }
        }

        if (signal?.aborted) {
          stream.end({ status: "aborted", appendedMessages: [], stopReason: "abort" });
          return;
        }

        // Build assistant message
        const content: Message["content"] =
          spec.content ??
          spec.deltas
            ?.filter((d) => d.type === "text")
            .map((d) => d.text)
            .join("") ??
          "Faux response";

        const assistantMessage: Message = {
          role: "assistant",
          content: Array.isArray(content) ? content : [{ type: "text", text: content as string }],
          timestamp: Date.now(),
        } as Message;

        // Add tool calls if specified
        if (
          spec.toolCalls &&
          spec.toolCalls.length > 0 &&
          Array.isArray(assistantMessage.content)
        ) {
          for (const tc of spec.toolCalls) {
            (assistantMessage.content as Array<Record<string, unknown>>).push({
              type: "toolCall",
              id: tc.id,
              name: tc.name,
              arguments: tc.arguments,
            });
          }
        }

        stream.push({ type: "message_end", message: assistantMessage });
        stream.push({ type: "step_end" });

        const status = spec.status ?? (spec.toolCalls?.length ? "continue" : "completed");
        const appendedMessages = spec.appendedMessages ?? [assistantMessage];

        stream.end({
          status,
          appendedMessages,
          stopReason: (spec.stopReason as ModelStepResult["stopReason"]) ?? "assistant",
        } as ModelStepResult);
      })();

      return stream;
    },

    async shutdown(): Promise<void> {},
  };
}

// ---- Simple task result builder ----

export function buildTaskResult(summary: string): AgentTaskResult {
  return { summary };
}

// ---- Sleep helper ----

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
