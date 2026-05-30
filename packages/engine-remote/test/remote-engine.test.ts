import { describe, it, expect } from "vitest";
import type {
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineEventEnvelope,
} from "piko-engine-protocol";
import { createRemoteEngine } from "../src/remote-engine.js";
import type { RemoteTransport } from "../src/protocol.js";
import { REMOTE_METHODS } from "../src/protocol.js";

function createFakeTransport(
  responseMap?: Map<string, unknown>,
): RemoteTransport & {
  sentMessages: { method: string; params: unknown }[];
  emitNotification: (method: string, params: unknown) => void;
} {
  const sentMessages: { method: string; params: unknown }[] = [];
  const handlers: ((method: string, params: unknown) => void)[] = [];

  return {
    sentMessages,

    async send(method: string, params: unknown): Promise<unknown> {
      sentMessages.push({ method, params });
      if (responseMap?.has(method)) {
        return responseMap.get(method);
      }
      return {};
    },

    onNotification(
      handler: (method: string, params: unknown) => void,
    ): () => void {
      handlers.push(handler);
      return () => {
        const idx = handlers.indexOf(handler);
        if (idx >= 0) handlers.splice(idx, 1);
      };
    },

    emitNotification(method: string, params: unknown) {
      for (const h of handlers) {
        h(method, params);
      }
    },

    async close(): Promise<void> {
      handlers.length = 0;
    },
  };
}

function buildTestInput(): EngineInput {
  return {
    runId: "test-run",
    stepId: "test-step",
    transcript: [
      { role: "user", content: "Hello", timestamp: Date.now() },
    ],
    systemPrompt: "Be helpful",
    model: {
      id: "test-model",
      name: "Test",
      api: "openai-completions",
      provider: "openai",
      baseUrl: "https://api.openai.com/v1",
      reasoning: false,
      input: ["text"],
      contextWindow: 128000,
      maxTokens: 16384,
    },
    provider: {},
    tools: [],
    settings: {
      maxSteps: 10,
      parallelTools: false,
      allowToolCalls: true,
      allowApprovals: false,
    },
  };
}

describe("RemoteEngine", () => {
  it("should send execute_step and receive result", async () => {
    const fakeResult: EngineStepResult = {
      status: "completed",
      appendedMessages: [
        {
          role: "assistant",
          content: [{ type: "text", text: "Hello back!" }],
          timestamp: Date.now(),
        },
      ],
      stopReason: "assistant",
    };

    const responseMap = new Map<string, unknown>();
    responseMap.set(REMOTE_METHODS.EXECUTE_STEP, fakeResult);

    const transport = createFakeTransport(responseMap);
    const engine = createRemoteEngine({ transport });

    const input = buildTestInput();
    const stream = engine.executeStep(input);

    // Collect events and result
    const events: EngineEvent[] = [];
    for await (const event of stream) {
      events.push(event);
    }
    const result = await stream.result();

    expect(transport.sentMessages).toHaveLength(1);
    expect(transport.sentMessages[0].method).toBe(
      REMOTE_METHODS.EXECUTE_STEP,
    );
    expect(result.status).toBe("completed");
    expect(result.appendedMessages).toHaveLength(1);
  });

  it("should forward engine/event notifications", async () => {
    let resolveExecute: (value: unknown) => void;
    const executePromise = new Promise<unknown>((resolve) => {
      resolveExecute = resolve;
    });

    const fakeResult: EngineStepResult = {
      status: "completed",
      appendedMessages: [],
      stopReason: "assistant",
    };

    const transport = createFakeTransport();
    // Override send for execute_step to delay resolution
    const originalSend = transport.send.bind(transport);
    transport.send = async (method: string, params: unknown) => {
      if (method === REMOTE_METHODS.EXECUTE_STEP) {
        return executePromise;
      }
      return originalSend(method, params);
    };

    const engine = createRemoteEngine({ transport });

    const input = buildTestInput();
    const stream = engine.executeStep(input);

    // Emit notification before resolving
    const envelope: EngineEventEnvelope = {
      runId: "test-run",
      stepId: "test-step",
      event: { type: "step_start" },
    };
    transport.emitNotification(REMOTE_METHODS.EVENT, envelope);

    // Now resolve
    resolveExecute!(fakeResult);

    const events: EngineEvent[] = [];
    for await (const event of stream) {
      events.push(event);
    }
    const result = await stream.result();

    expect(events.some((e) => e.type === "step_start")).toBe(true);
    expect(result.status).toBe("completed");
  });

  it("should send resolve_approval", async () => {
    const fakeResult: EngineStepResult = {
      status: "completed",
      appendedMessages: [],
      stopReason: "approval",
    };

    const responseMap = new Map<string, unknown>();
    responseMap.set(REMOTE_METHODS.RESOLVE_APPROVAL, fakeResult);

    const transport = createFakeTransport(responseMap);
    const engine = createRemoteEngine({ transport });

    const result = await engine.resolveApproval?.({
      runId: "test-run",
      stepId: "test-step",
      approvalRequestId: "req-1",
      decision: "accept",
      transcript: [],
    });

    expect(result).toBeDefined();
    expect(result!.status).toBe("completed");
    expect(transport.sentMessages[0].method).toBe(
      REMOTE_METHODS.RESOLVE_APPROVAL,
    );
  });
});
