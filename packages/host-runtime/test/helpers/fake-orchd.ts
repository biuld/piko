import type {
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  ApprovalGateway,
  HostEvent,
  HostEventListener,
  Message,
  Orchestrator,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  ToolProvider,
  ToolSet,
} from "../../src/orchd/protocol/index.js";

type ResponseFactory = (prompt: string, agentId: string) => Message;

export class FakeOrchd implements Orchestrator {
  private listeners = new Set<HostEventListener>();
  private responses: ResponseFactory[] = [];
  private agents = new Map<string, AgentSpec>();
  private tasks = new Map<string, AgentTask & { id: string }>();
  private taskResults = new Map<string, OrchRunResult>();
  private seq = 0;

  constructor(responses: Array<Message | ResponseFactory> = []) {
    this.setResponses(responses);
  }

  setResponses(responses: Array<Message | ResponseFactory>): void {
    this.responses = responses.map((response) =>
      typeof response === "function" ? response : () => response,
    );
  }

  pushResponse(response: Message | ResponseFactory): void {
    this.responses.push(typeof response === "function" ? response : () => response);
  }

  registerAgent(spec: AgentSpec): void {
    this.agents.set(spec.id, spec);
  }

  unregisterAgent(agentId: string): void {
    this.agents.delete(agentId);
  }

  registerToolSet(_toolSet: ToolSet): void {}

  unregisterToolSet(_toolSetId: string): void {}

  setModelConfig(_config: OrchModelConfig): void {}

  setApprovalGateway(_gateway: ApprovalGateway | undefined): void {}

  registerProvider(_provider: ToolProvider): void {}

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    return this.dispatchDetached(task);
  }

  async dispatchDetached(task: AgentTask): Promise<AgentTaskId> {
    const taskId = task.id ?? this.nextTaskId();
    const stored = { ...task, id: taskId };
    this.tasks.set(taskId, stored);
    const result = await this.runTask(stored);
    this.taskResults.set(taskId, result);
    return taskId;
  }

  async joinTask(taskId: string): Promise<unknown> {
    const result = this.taskResults.get(taskId);
    if (!result) throw new Error("Detached task not found");
    return result;
  }

  async run(prompt: string, opts: OrchRunOptions = {}): Promise<OrchRunResult> {
    const agentId = opts.targetAgentId ?? "main";
    const task: AgentTask & { id: string } = {
      id: this.nextTaskId(),
      targetAgentId: agentId,
      prompt,
      source: { type: "user" },
      history: opts.history,
    };
    return this.runTask(task);
  }

  async cancelTask(_taskId: string, _reason?: string): Promise<void> {}

  subscribe(listener: HostEventListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  snapshot(): OrchState {
    const agents: Record<string, AgentRuntimeState> = {};
    for (const [id, spec] of this.agents) {
      agents[id] = { id, spec, status: "idle", transcript: [] };
    }
    return {
      runId: "fake-orchd",
      status: "idle",
      toolSets: {},
      agents,
      tasks: {},
    };
  }

  updatePlan(_agentId: string, _taskId: string, _plan: unknown[]): void {}

  async getGraph(): Promise<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }> {
    return { nodes: [], edges: [] };
  }

  async llmCall(_params: {
    model: any;
    systemPrompt?: string;
    messages: Message[];
    settings?: any;
  }): Promise<{ text: string }> {
    const factory =
      this.responses.shift() ?? (() => assistantText("Canned fake-orchd LLM response"));
    const msg = factory("", "llmCall");
    let text = "";
    if (msg.role === "assistant") {
      text = msg.content
        .filter((c): c is { type: "text"; text: string } => c.type === "text")
        .map((c) => c.text)
        .join("\n");
    }
    return { text: text || "Canned fake-orchd LLM response" };
  }

  private async runTask(task: AgentTask & { id: string }): Promise<OrchRunResult> {
    const agentId = task.targetAgentId;
    const history = task.history ?? [];
    const user: Message = {
      role: "user",
      content: task.prompt,
      timestamp: Date.now(),
    };
    const assistant = this.nextResponse(task.prompt, agentId);
    const messages = [...history, user, assistant];

    if (agentId !== "main") {
      this.emit({ type: "task_created", task });
      this.emit({ type: "task_started", agentId, taskId: task.id });
      this.emit({
        type: "task_transcript_committed",
        agentId,
        taskId: task.id,
        messages,
        summary: "",
        finalStatus: "completed",
      });
      this.emit({
        type: "task_completed",
        agentId,
        taskId: task.id,
        result: { summary: "" },
      });
    }

    return {
      messages,
      totalSteps: 1,
      status: "completed",
    };
  }

  private nextResponse(prompt: string, agentId: string): Message {
    const factory = this.responses.shift() ?? ((p) => assistantText(`Response: ${p}`));
    return factory(prompt, agentId);
  }

  private nextTaskId(): string {
    this.seq += 1;
    return `fake-task-${this.seq}`;
  }

  private emit(event: HostEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}

export function assistantText(text: string): Message {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "openai-completions",
    provider: "fake-orchd",
    model: "fake-orchd-model",
    usage: emptyUsage(),
    stopReason: "stop",
    timestamp: Date.now(),
  } as Message;
}

export function assistantContent(content: unknown[]): Message {
  return {
    role: "assistant",
    content,
    api: "openai-completions",
    provider: "fake-orchd",
    model: "fake-orchd-model",
    usage: emptyUsage(),
    stopReason: "stop",
    timestamp: Date.now(),
  } as Message;
}

function emptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}
