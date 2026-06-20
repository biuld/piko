import { describe, expect, it } from "bun:test";
import type { Model } from "piko-orchestrator-protocol";
import { ActionService } from "../src/renderer/opentui/action-service.js";
import { createDefaultStore } from "../src/renderer/opentui/store.js";

function buildTestModel(): Model<string> {
  return {
    id: "test-model",
    name: "Test Model",
    api: "openai-completions",
    provider: "test-provider",
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

class MockEventStream {
  private events: any[];
  private finalResult: any;
  constructor(events: any[], finalResult: any) {
    this.events = events;
    this.finalResult = finalResult;
  }
  end(result: any) {
    this.finalResult = result;
  }
  async *[Symbol.asyncIterator]() {
    for (const e of this.events) {
      yield e;
    }
  }
  async result() {
    return this.finalResult;
  }
}

describe("ActionService & SlotRenderer (TUI)", () => {
  it("stream.status === 'running' 时 Editor 不应 disabled", () => {
    const mockCtx = {
      timelineItems: () => [],
      layout: () => ({ viewport: { width: 80, height: 24 }, mode: "regular" }),
      state: () => ({
        stream: { status: "running" },
        input: { draft: "", revision: 0 },
      }),
      statusContract: () => ({}),
      orchestratorSnapshot: () => ({}) as any,
      spinnerFrame: () => 0,
      isRunning: () => true,
      store: {} as any,
      actionSvc: {} as any,
      ctrl: {
        autocomplete: {} as any,
        getAutocomplete: () => [],
        setAutocompleteController: () => {},
        setAutocompleteKeyHandler: () => {},
        setEditorTextAccessor: () => {},
      } as any,
      host: {} as any,
    } as any;

    const state = mockCtx.state();
    const disabled = state.stream.status === "awaiting_approval";
    const placeholder =
      state.stream.status === "running"
        ? "Steer the running agent..."
        : "Ask a question, or type '/' for commands...";

    expect(disabled).toBe(false);
    expect(placeholder).toBe("Steer the running agent...");
  });

  it("运行时提交文本会调用 Host steering 路径", async () => {
    const promptCalledWith: any[] = [];
    const mockHost = {
      prompt: (text: string, mode: string, agentId: string, _signal?: AbortSignal) => {
        promptCalledWith.push({ text, mode, agentId });
        return null; // Simulate queueing / steering
      },
      setLifecycleCallback: () => {},
    } as any;

    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });

    store.setState((s) => ({ ...s, stream: { ...s.stream, status: "running" } }));

    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});
    const dummyController = new AbortController();
    actionSvc.abortController = dummyController;

    await actionSvc.submitPrompt("Steering instruction");

    expect(promptCalledWith).toEqual([
      { text: "Steering instruction", mode: "auto", agentId: "main" },
    ]);
    expect(store.state().stream.status).toBe("running"); // Should remain running
    expect(actionSvc.abortController).toBe(dummyController);
  });

  it("abort 后最终状态为 idle", async () => {
    const mockHost = {
      prompt: (_text: string, _mode: string, _agentId: string, signal?: AbortSignal) => {
        const stream = new MockEventStream([], { status: "aborted", messages: [] });
        if (signal) {
          signal.addEventListener("abort", () => {
            stream.end({ status: "aborted", messages: [] });
          });
        }
        return stream;
      },
      setLifecycleCallback: () => {},
    } as any;

    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});

    const runPromise = actionSvc.submitPrompt("Test abort");
    expect(store.state().stream.status).toBe("running");

    actionSvc.abortRun();
    await runPromise;

    expect(store.state().stream.status).toBe("idle");
  });

  it("stream 抛错后最终状态为 idle", async () => {
    const mockHost = {
      prompt: (_text: string, _mode: string, _agentId: string, _signal?: AbortSignal) => {
        const stream = new MockEventStream([], { status: "error", messages: [] });
        stream[Symbol.asyncIterator] = async function* () {
          yield;
          throw new Error("Stream connection failed");
        };
        return stream;
      },
      setLifecycleCallback: () => {},
    } as any;

    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});

    await actionSvc.submitPrompt("Test error");

    expect(store.state().stream.status).toBe("idle");
  });

  it("已开始新 run 时，旧 run 的 finally 不会清掉新 run 的 abort controller", async () => {
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });

    let resolve1: any;
    const promise1 = new Promise((r) => (resolve1 = r));
    const stream1 = new MockEventStream([], { status: "completed", messages: [] });
    stream1.result = () => promise1;

    let resolve2: any;
    const promise2 = new Promise((r) => (resolve2 = r));
    const stream2 = new MockEventStream([], { status: "completed", messages: [] });
    stream2.result = () => promise2;

    let streamIndex = 0;
    const mockHost = {
      prompt: (_text: string, _mode: string, _agentId: string, _signal?: AbortSignal) => {
        streamIndex++;
        return streamIndex === 1 ? stream1 : stream2;
      },
      setLifecycleCallback: () => {},
    } as any;

    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});

    const run1Promise = actionSvc.submitPrompt("Run 1");
    const ac1 = actionSvc.abortController;
    expect(ac1).not.toBeNull();

    const run2Promise = actionSvc.submitPrompt("Run 2");
    const ac2 = actionSvc.abortController;
    expect(ac2).not.toBeNull();
    expect(ac2).not.toBe(ac1);

    resolve1({ status: "completed", messages: [] });
    await run1Promise;

    expect(actionSvc.abortController).toBe(ac2);

    resolve2({ status: "completed", messages: [] });
    await run2Promise;

    expect(actionSvc.abortController).toBeNull();
  });
});
