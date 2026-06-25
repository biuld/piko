import type {
  AgentSpec,
  AgentTask,
  AgentTaskId,
  ApprovalGateway,
  HostEvent,
  HostEventListener,
  JsonRpcFailure,
  JsonRpcId,
  JsonRpcMessage,
  JsonRpcNotification,
  JsonRpcRequest,
  JsonRpcSuccess,
  Orchestrator,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  ToolProvider,
  ToolSet,
} from "piko-orch-protocol";

type Pending = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
};

const emptyState = (): OrchState => ({
  runId: "",
  status: "idle",
  toolSets: {},
  agents: {},
  tasks: {},
});

export class OrchdRpcClient implements Orchestrator {
  private proc?: Bun.Subprocess<"pipe", "pipe", "pipe">;
  private nextId = 1;
  private pending = new Map<JsonRpcId, Pending>();
  private listeners = new Set<HostEventListener>();
  private providers = new Map<string, ToolProvider>();
  private approvalGateway?: ApprovalGateway;
  private state = emptyState();
  private ready: Promise<void>;
  private writeQueue: Promise<void> = Promise.resolve();

  constructor(
    private readonly options: {
      command?: string[];
      cwd?: string;
    } = {},
  ) {
    this.ready = this.start();
  }

  registerAgent(spec: AgentSpec): void {
    this.enqueue(async () => {
      await this.call("orch.register_agent", spec);
      this.state.agents[spec.id] = {
        id: spec.id,
        spec,
        status: "idle",
        transcript: [],
      };
    });
  }

  unregisterAgent(agentId: string): void {
    this.enqueue(async () => {
      await this.call("orch.unregister_agent", { agentId });
      delete this.state.agents[agentId];
    });
  }

  registerToolSet(toolSet: ToolSet): void {
    this.enqueue(async () => {
      await this.call("orch.register_tool_set", toolSet);
      this.state.toolSets[toolSet.id] = toolSet;
    });
  }

  unregisterToolSet(toolSetId: string): void {
    this.enqueue(async () => {
      await this.call("orch.unregister_tool_set", { toolSetId });
      delete this.state.toolSets[toolSetId];
    });
  }

  setModelConfig(config: OrchModelConfig): void {
    this.enqueue(() => this.call("orch.set_model_config", config).then(() => undefined));
  }

  setApprovalGateway(gateway: ApprovalGateway | undefined): void {
    this.approvalGateway = gateway;
  }

  registerProvider(provider: ToolProvider): void {
    this.providers.set(provider.id, provider);
    this.enqueue(() =>
      this.call("orch.register_tool_provider", {
        providerId: provider.id,
        source: provider.source,
      }).then(() => undefined),
    );
  }

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    return this.dispatchDetached(task);
  }

  async dispatchDetached(task: AgentTask): Promise<AgentTaskId> {
    await this.ready;
    const result = await this.call<{ taskId: AgentTaskId }>("orch.start_task", task);
    return result.taskId;
  }

  async joinTask(taskId: string): Promise<unknown> {
    await this.ready;
    return this.call("orch.await_task", { taskId });
  }

  async run(prompt: string, opts: OrchRunOptions = {}): Promise<OrchRunResult> {
    await this.ready;
    const task: AgentTask = {
      targetAgentId: opts.targetAgentId ?? "main",
      prompt,
      source: { type: "user" },
      history: opts.history,
    };

    const { taskId } = await this.call<{ taskId: string }>("orch.start_task", task);
    const abort = () => {
      void this.call("orch.cancel_task", { taskId, reason: "AbortSignal aborted" }).catch(() => {});
    };
    if (opts.signal?.aborted) abort();
    opts.signal?.addEventListener("abort", abort, { once: true });
    try {
      const raw = await this.call<any>("orch.await_task", { taskId });
      return normalizeRunResult(raw, opts.signal?.aborted ?? false);
    } finally {
      opts.signal?.removeEventListener("abort", abort);
    }
  }

  async cancelTask(taskId: string, reason?: string): Promise<void> {
    await this.ready;
    await this.call("orch.cancel_task", { taskId, reason });
  }

  subscribe(listener: HostEventListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  snapshot(): OrchState {
    void this.ready
      .then(() => this.call<OrchState>("orch.snapshot", {}))
      .then((state) => {
        this.state = state;
      })
      .catch(() => {});
    return this.state;
  }

  updatePlan(agentId: string, taskId: string, plan: unknown[]): void {
    this.enqueue(() =>
      this.call("orch.update_plan", { agentId, taskId, plan }).then(() => undefined),
    );
  }

  async getGraph(): Promise<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }> {
    await this.ready;
    return this.call("orch.get_graph", {});
  }

  async dispose(): Promise<void> {
    this.proc?.kill();
    this.proc = undefined;
    for (const pending of this.pending.values()) {
      pending.reject(new Error("orchd RPC client closed"));
    }
    this.pending.clear();
  }

  private enqueue(work: () => Promise<void>): void {
    this.ready = this.ready.then(work, work);
  }

  private async start(): Promise<void> {
    const command = this.options.command ?? defaultOrchdCommand();
    this.proc = Bun.spawn(command, {
      cwd: this.options.cwd ?? process.cwd(),
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    void this.readLoop(this.proc.stdout);
    void this.logStderr(this.proc.stderr);
    await this.call("orch.subscribe_events", {});
  }

  private async call<T = unknown>(method: string, params: unknown): Promise<T> {
    const id = this.nextId++;
    const message: JsonRpcRequest = { jsonrpc: "2.0", id, method, params };
    const result = new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: (value) => resolve(value as T), reject });
    });
    await this.write(message);
    return result;
  }

  private async write(message: JsonRpcRequest | JsonRpcNotification): Promise<void> {
    this.writeQueue = this.writeQueue.then(async () => {
      const stdin = this.proc?.stdin;
      if (!stdin) throw new Error("orchd stdin is not available");
      const line = `${JSON.stringify(message)}\n`;
      await stdin.write(new TextEncoder().encode(line));
    });
    return this.writeQueue;
  }

  private async readLoop(stdout: ReadableStream<Uint8Array>): Promise<void> {
    const reader = stdout.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let newline = buffer.indexOf("\n");
        while (newline >= 0) {
          const line = buffer.slice(0, newline).trim();
          buffer = buffer.slice(newline + 1);
          if (line) void this.handleLine(line);
          newline = buffer.indexOf("\n");
        }
      }
    } finally {
      for (const pending of this.pending.values()) pending.reject(new Error("orchd exited"));
      this.pending.clear();
    }
  }

  private async logStderr(stderr: ReadableStream<Uint8Array>): Promise<void> {
    for await (const _chunk of stderr) {
      // Keep stderr drained. Host/TUI logging policy can be added above this layer.
    }
  }

  private async handleLine(line: string): Promise<void> {
    let message: JsonRpcMessage;
    try {
      message = JSON.parse(line) as JsonRpcMessage;
    } catch {
      return;
    }

    if ("id" in message && ("result" in message || "error" in message)) {
      this.handleResponse(message as JsonRpcSuccess | JsonRpcFailure);
      return;
    }

    if ("method" in message && "id" in message) {
      await this.handleRequest(message as JsonRpcRequest);
      return;
    }

    if ("method" in message) {
      this.handleNotification(message as JsonRpcNotification);
    }
  }

  private handleResponse(message: JsonRpcSuccess | JsonRpcFailure): void {
    if (message.id === null) return;
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    if ("error" in message) pending.reject(new Error(message.error.message));
    else pending.resolve(message.result);
  }

  private async handleRequest(message: JsonRpcRequest): Promise<void> {
    try {
      const result = await this.dispatchHostRequest(message.method, message.params);
      await this.writeRaw({ jsonrpc: "2.0", id: message.id, result });
    } catch (error) {
      await this.writeRaw({
        jsonrpc: "2.0",
        id: message.id,
        error: {
          code: -32603,
          message: error instanceof Error ? error.message : String(error),
        },
      });
    }
  }

  private async writeRaw(message: JsonRpcSuccess | JsonRpcFailure): Promise<void> {
    this.writeQueue = this.writeQueue.then(async () => {
      const stdin = this.proc?.stdin;
      if (!stdin) throw new Error("orchd stdin is not available");
      await stdin.write(new TextEncoder().encode(`${JSON.stringify(message)}\n`));
    });
    return this.writeQueue;
  }

  private handleNotification(message: JsonRpcNotification): void {
    if (message.method === "host_event") {
      const params = message.params as { event?: HostEvent } | HostEvent | undefined;
      const event =
        params && typeof params === "object" && "event" in params ? params.event : params;
      for (const listener of this.listeners) listener(event as HostEvent);
    }
  }

  private async dispatchHostRequest(method: string, params: unknown): Promise<unknown> {
    switch (method) {
      case "host.tools.discover": {
        const { providerId, context } = params as any;
        const provider = this.providers.get(providerId);
        if (!provider) throw new Error(`Unknown host tool provider: ${providerId}`);
        return provider.discover(context);
      }
      case "host.tools.execute": {
        const { providerId, call, context } = params as any;
        const provider = this.providers.get(providerId);
        if (!provider) throw new Error(`Unknown host tool provider: ${providerId}`);
        return provider.execute(call, context);
      }
      case "host.approval.request": {
        const { request } = params as any;
        if (!this.approvalGateway) return "decline";
        return this.approvalGateway.requestToolApproval(request);
      }
      default:
        throw new Error(`Method not found: ${method}`);
    }
  }
}

function normalizeRunResult(raw: any, aborted: boolean): OrchRunResult {
  if (raw && Array.isArray(raw.messages)) {
    return {
      messages: raw.messages,
      totalSteps: raw.totalSteps ?? raw.total_steps ?? 0,
      status: aborted ? "aborted" : (raw.status ?? "completed"),
    };
  }
  return {
    messages: [],
    totalSteps: 0,
    status: aborted ? "aborted" : "error",
  };
}

function defaultOrchdCommand(): string[] {
  if (process.env.PIKO_ORCHD_PATH) return [process.env.PIKO_ORCHD_PATH];
  return [
    "cargo",
    "run",
    "--quiet",
    "--manifest-path",
    "packages/orchd/Cargo.toml",
    "--bin",
    "orchd",
  ];
}
