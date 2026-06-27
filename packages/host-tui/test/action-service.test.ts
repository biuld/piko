import { describe, expect, it } from "bun:test";
import type { Model } from "piko-host-runtime";
import { HostdClient, type HostdTransport } from "../src/client/index.js";
import { ActionService } from "../src/renderer/opentui/action-service.js";
import { createDefaultStore } from "../src/renderer/opentui/store.js";
import { TuiController } from "../src/runtime/tui-controller.js";

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

class AutoHostdTransport implements HostdTransport {
  private lineListeners = new Set<(line: string) => void>();
  private closeListeners = new Set<(code?: number | null) => void>();
  private sessionId = "session-hostd";
  readonly commands: any[] = [];

  constructor(private readonly completeTurns = true) {}

  write(line: string): void {
    const command = JSON.parse(line);
    this.commands.push(command);
    this.emit({ type: "command_accepted", command_id: command.command_id });
    if (command.type === "session_create") {
      this.emit({
        type: "session_created",
        session_id: this.sessionId,
        cwd: command.cwd,
        timestamp: Date.now(),
      });
    } else if (command.type === "turn_submit") {
      this.emit({
        type: "turn_started",
        session_id: command.session_id,
        turn_id: "turn-hostd",
        root_task_id: "task-hostd",
        timestamp: Date.now(),
      });
      this.emit({
        type: "text_delta",
        task_id: "task-hostd",
        agent_id: "main",
        message_id: "msg-hostd",
        delta: "hostd says hi",
      });
      if (this.completeTurns) {
        this.emit({
          type: "turn_completed",
          session_id: command.session_id,
          turn_id: "turn-hostd",
          total_tasks: 1,
          timestamp: Date.now(),
        });
      }
    } else if (command.type === "approval_respond") {
      this.emit({
        type: "approval_resolved",
        task_id: "task-hostd",
        agent_id: "main",
        approval_id: command.approval_id,
        decision: command.decision,
      });
    } else if (command.type === "turn_cancel") {
      this.emit({
        type: "turn_cancelled",
        session_id: command.session_id,
        turn_id: command.turn_id,
        timestamp: Date.now(),
      });
    }
  }

  onLine(listener: (line: string) => void): void {
    this.lineListeners.add(listener);
  }

  onClose(listener: (code?: number | null) => void): void {
    this.closeListeners.add(listener);
  }

  close(): void {
    for (const listener of this.closeListeners) listener(0);
  }

  private emit(event: unknown): void {
    queueMicrotask(() => {
      for (const listener of this.lineListeners) listener(JSON.stringify(event));
    });
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

  it("hostd client enabled 时提交文本只走 hostd HostEvent 路径", async () => {
    const promptCalledWith: any[] = [];
    const mockHost = {
      prompt: (...args: any[]) => {
        promptCalledWith.push(args);
        return null;
      },
      setLifecycleCallback: () => {},
    } as any;
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});
    actionSvc.setHostdClient(
      new HostdClient({
        transport: new AutoHostdTransport(),
        commandTimeoutMs: 100,
      }),
    );

    await actionSvc.submitPrompt("hello hostd");
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(promptCalledWith).toEqual([]);
    expect(store.state().stream.status).toBe("idle");
    expect(store.state().transcript.some((message) => message.text.includes("hostd says hi"))).toBe(
      true,
    );
  });

  it("hostd approval resolve 会发送 approval_respond", async () => {
    const transport = new AutoHostdTransport();
    const mockHost = {
      sessionId: "session-hostd",
      setLifecycleCallback: () => {},
    } as any;
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    store.dispatch({ type: "session_info_updated", sessionId: "session-hostd" });
    store.dispatch({
      type: "approval_needed",
      toolEntityId: "approval-1",
      callId: "approval-1",
      toolName: "bash",
      toolArgs: { command: "date" },
    });

    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});
    actionSvc.setHostdClient(
      new HostdClient({
        transport,
        commandTimeoutMs: 100,
      }),
    );

    actionSvc.resolveApproval("approval-1", "accept");
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(transport.commands.some((command) => command.type === "approval_respond")).toBe(true);
    expect(transport.commands.find((command) => command.type === "approval_respond")).toMatchObject(
      {
        session_id: "session-hostd",
        approval_id: "approval-1",
        decision: "accept",
      },
    );
    expect(store.state().approval.pending).toBeUndefined();
  });

  it("hostd abort 会发送 turn_cancel", async () => {
    const transport = new AutoHostdTransport(false);
    const mockHost = {
      sessionId: "session-hostd",
      setLifecycleCallback: () => {},
    } as any;
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    const actionSvc = new ActionService(mockHost, store, {} as any, undefined, () => {});
    actionSvc.setHostdClient(
      new HostdClient({
        transport,
        commandTimeoutMs: 100,
      }),
    );

    await actionSvc.submitPrompt("hello hostd");
    await new Promise((resolve) => setTimeout(resolve, 0));
    actionSvc.abortRun();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(transport.commands.some((command) => command.type === "turn_cancel")).toBe(true);
    expect(transport.commands.find((command) => command.type === "turn_cancel")).toMatchObject({
      session_id: "session-hostd",
      turn_id: "turn-hostd",
    });
  });

  it("hostd model/thinking changes 会发送 config_set", async () => {
    const transport = new AutoHostdTransport();
    const model = buildTestModel();
    const providerConfig = { apiKey: "test-key" } as any;
    let currentConfig = { model, provider: providerConfig, settings: {} } as any;
    const mockHost = {
      sessionId: "session-hostd",
      getConfig: () => currentConfig,
      setConfig: (next: any) => {
        currentConfig = next;
      },
      setThinkingLevel: () => {},
      setLifecycleCallback: () => {},
    } as any;
    const settingsManager = {
      setDefaultModelAndProvider: () => {},
      setDefaultThinkingLevel: () => {},
    } as any;
    const modelRegistry = {
      resolve: () => ({ model, providerConfig }),
    } as any;
    const store = createDefaultStore(model, providerConfig, "/test/cwd", {
      hideThinking: false,
      theme: "dark",
    });
    const actionSvc = new ActionService(mockHost, store, settingsManager, modelRegistry, () => {});
    actionSvc.setHostdClient(
      new HostdClient({
        transport,
        commandTimeoutMs: 100,
      }),
    );

    expect(actionSvc.switchModel("test-model", "test-provider")).toBe(true);
    actionSvc.setThinkingLevel("high");
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(transport.commands.filter((command) => command.type === "config_set")).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          default_provider: "test-provider",
          default_model: "test-model",
        }),
        expect.objectContaining({
          default_thinking_level: "high",
        }),
      ]),
    );
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

  it("并行审批按 FIFO 展示并自动推进", () => {
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd");
    const mockHost = { setLifecycleCallback: () => {} } as any;
    const actionSvc = new ActionService(mockHost, store, {} as any);
    let listener!: (pending: any) => void;
    actionSvc.setApprovalBridge({
      onPending(next) {
        listener = next;
      },
    });
    const decisions: string[] = [];
    for (const [callId, toolName] of [
      ["call-1", "bash"],
      ["call-2", "edit"],
      ["call-3", "write"],
    ]) {
      listener({
        request: { callId, toolName, toolArgs: {}, taskId: "task-1", agentId: "main" },
        resolve: (decision: string) => decisions.push(`${callId}:${decision}`),
      });
    }

    expect(store.state().approval.pending?.callId).toBe("call-1");
    expect(store.state().approval.queue.map((item) => item.callId)).toEqual(["call-2", "call-3"]);

    actionSvc.resolveApproval("call-1", "accept");
    expect(store.state().approval.pending?.callId).toBe("call-2");
    actionSvc.resolveApproval("call-2", "decline");
    expect(store.state().approval.pending?.callId).toBe("call-3");
    actionSvc.resolveApproval("call-3", "accept");

    expect(store.state().approval.pending).toBeUndefined();
    expect(store.state().approval.queue).toEqual([]);
    expect(store.state().stream.status).toBe("running");
    expect(decisions).toEqual(["call-1:accept", "call-2:decline", "call-3:accept"]);
  });

  it("并行审批不会因 provider 复用 callId 而互相覆盖", () => {
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd");
    const actionSvc = new ActionService(
      { setLifecycleCallback: () => {} } as any,
      store,
      {} as any,
    );
    let listener!: (pending: any) => void;
    actionSvc.setApprovalBridge({
      onPending(next) {
        listener = next;
      },
    });
    const decisions: string[] = [];
    for (const toolEntityId of ["assistant-run-1:tool:0", "assistant-run-2:tool:0"]) {
      listener({
        request: {
          toolEntityId,
          callId: "reused",
          toolName: "bash",
          toolArgs: {},
          taskId: toolEntityId,
          agentId: "main",
        },
        resolve: (decision: string) => decisions.push(`${toolEntityId}:${decision}`),
      });
    }

    expect(store.state().approval.pending?.toolEntityId).toBe("assistant-run-1:tool:0");
    expect(store.state().approval.queue[0]?.toolEntityId).toBe("assistant-run-2:tool:0");

    actionSvc.resolveApproval("assistant-run-1:tool:0", "accept");
    actionSvc.resolveApproval("assistant-run-2:tool:0", "decline");

    expect(decisions).toEqual(["assistant-run-1:tool:0:accept", "assistant-run-2:tool:0:decline"]);
  });

  it("审批 panel 通过 Enter/Esc 处理当前队首", () => {
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd");
    const mockHost = { setLifecycleCallback: () => {} } as any;
    const resolved: Array<[string, string]> = [];
    const controller = new TuiController(mockHost, store, () => {});
    const mockSvc = {
      resolveApproval: (callId: string, decision: "accept" | "decline") => {
        resolved.push([callId, decision]);
        store.dispatch({ type: "approval_resolved", callId, decision });
      },
    } as any;
    controller.setActionService(mockSvc);
    store.dispatch({ type: "approval_needed", callId: "call-1", toolName: "bash", toolArgs: {} });
    mockSvc.onOpenApprovalSurface?.();

    expect(controller.handleKey({ name: "return", ctrl: false, shift: false })).toBe(true);
    expect(resolved).toEqual([["call-1", "accept"]]);

    store.dispatch({ type: "approval_needed", callId: "call-2", toolName: "edit", toolArgs: {} });
    mockSvc.onOpenApprovalSurface?.();
    expect(controller.handleKey({ name: "escape", ctrl: false, shift: false })).toBe(true);
    expect(resolved).toEqual([
      ["call-1", "accept"],
      ["call-2", "decline"],
    ]);
  });

  it("并行审批取消时可移除队首或队列项", () => {
    const store = createDefaultStore(buildTestModel(), {} as any, "/test/cwd");
    const actionSvc = new ActionService(
      { setLifecycleCallback: () => {} } as any,
      store,
      {} as any,
    );
    let listener!: (pending: any) => void;
    actionSvc.setApprovalBridge({
      onPending(next) {
        listener = next;
      },
    });
    const first = new AbortController();
    const second = new AbortController();
    listener({
      request: {
        callId: "call-1",
        toolName: "bash",
        toolArgs: {},
        taskId: "task-1",
        agentId: "main",
      },
      resolve: () => {},
      signal: first.signal,
    });
    listener({
      request: {
        callId: "call-2",
        toolName: "edit",
        toolArgs: {},
        taskId: "task-1",
        agentId: "main",
      },
      resolve: () => {},
      signal: second.signal,
    });

    second.abort();
    expect(store.state().approval.pending?.callId).toBe("call-1");
    expect(store.state().approval.queue).toEqual([]);

    first.abort();
    expect(store.state().approval.pending).toBeUndefined();
    expect(store.state().stream.status).toBe("running");
  });
});
