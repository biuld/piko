// ---- pi-ai re-exports ----
export type {
  AssistantMessage,
  ImageContent,
  KnownProvider,
  Message,
  TextContent,
  ThinkingContent,
  ToolCall,
  ToolResultMessage,
  UserMessage,
} from "@earendil-works/pi-ai";
export { getEnvApiKey, getModel, getModels, getProviders } from "@earendil-works/pi-ai";

// ---- Token usage ----
export interface TokenUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  totalTokens: number;
  total: number;
  cost: { input: number; output: number; cacheRead: number; cacheWrite: number; total: number };
}

// ---- Engine model ----
export interface EngineModel {
  id: string;
  name: string;
  api: string;
  provider: string;
  baseUrl: string;
  reasoning: boolean;
  input: ("text" | "image")[];
  contextWindow: number;
  maxTokens: number;
}

// ---- EventStream ----
export class EventStream<T, R = T> implements AsyncIterable<T> {
  private queue: T[] = [];
  private waiting: ((value: IteratorResult<T>) => void)[] = [];
  private done = false;
  private finalResultPromise: Promise<R>;
  private resolveFinalResult!: (result: R) => void;

  constructor() {
    this.finalResultPromise = new Promise((resolve) => {
      this.resolveFinalResult = resolve;
    });
  }

  push(event: T): void {
    if (this.done) return;
    const waiter = this.waiting.shift();
    if (waiter) waiter({ value: event, done: false });
    else this.queue.push(event);
  }

  end(result: R): void {
    this.done = true;
    this.resolveFinalResult(result);
    for (const waiter of this.waiting) waiter({ value: undefined as unknown as T, done: true });
    this.waiting.length = 0;
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    while (true) {
      if (this.queue.length > 0) {
        yield this.queue.shift()!;
        continue;
      }
      if (this.done) return;
      const r = await new Promise<IteratorResult<T>>((resolve) => this.waiting.push(resolve));
      if (r.done) return;
      yield r.value;
    }
  }

  result(): Promise<R> {
    return this.finalResultPromise;
  }
}
