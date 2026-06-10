// ---- Tool Handler Registry ----

// ============================================================
// Types
// ============================================================

export interface ToolContext {
  agentId: string;
  taskId: string;
  emit: (event: Record<string, unknown> & { type: string }) => void;
  signal?: AbortSignal;
}

export type ToolHandler = (args: Record<string, unknown>, ctx: ToolContext) => Promise<unknown>;

export type ResourceHandler = (
  request: {
    agentId: string;
    taskId: string;
    toolCalls: Array<{ id: string; name: string; args: Record<string, unknown> }>;
  },
  ctx: ToolContext,
) => Promise<{ proceed: true } | { decline: true; reason?: string }>;

export type RenderHandler = (event: Record<string, unknown> & { type: string }) => void;

// ============================================================
// Registry
// ============================================================

export interface ToolRegistry {
  /** Register a tool executor by name. */
  registerTool(name: string, handler: ToolHandler): () => void;

  /** Register a render event handler by event type. */
  registerRender(type: string, handler: RenderHandler): () => void;

  /** Register a resource gate (approval, rate-limit, sandbox check). */
  registerResource(handler: ResourceHandler): () => void;

  /** Execute a tool by name. */
  executeTool(name: string, args: Record<string, unknown>, ctx: ToolContext): Promise<unknown>;

  /** Run resource gates. Returns whether execution should proceed. */
  checkResource(
    request: {
      agentId: string;
      taskId: string;
      toolCalls: Array<{ id: string; name: string; args: Record<string, unknown> }>;
    },
    ctx: ToolContext,
  ): Promise<{ proceed: true } | { decline: true; reason?: string }>;

  /** Dispatch a render event to matching handlers. */
  dispatchRender(type: string, event: Record<string, unknown> & { type: string }): void;

  /** Get all registered tool names. */
  toolNames(): string[];
}

export function createToolRegistry(): ToolRegistry {
  const toolHandlers = new Map<string, ToolHandler[]>();
  const renderHandlers = new Map<string, RenderHandler[]>();
  const resourceHandlers: ResourceHandler[] = [];

  return {
    registerTool(name: string, handler: ToolHandler): () => void {
      const list = toolHandlers.get(name) ?? [];
      list.push(handler);
      toolHandlers.set(name, list);
      return () => {
        const idx = list.indexOf(handler);
        if (idx >= 0) list.splice(idx, 1);
      };
    },

    registerRender(type: string, handler: RenderHandler): () => void {
      const list = renderHandlers.get(type) ?? [];
      list.push(handler);
      renderHandlers.set(type, list);
      return () => {
        const idx = list.indexOf(handler);
        if (idx >= 0) list.splice(idx, 1);
      };
    },

    registerResource(handler: ResourceHandler): () => void {
      resourceHandlers.push(handler);
      return () => {
        const idx = resourceHandlers.indexOf(handler);
        if (idx >= 0) resourceHandlers.splice(idx, 1);
      };
    },

    async executeTool(
      name: string,
      args: Record<string, unknown>,
      ctx: ToolContext,
    ): Promise<unknown> {
      const list = toolHandlers.get(name);
      if (!list || list.length === 0) {
        throw new Error(`No handler registered for tool: ${name}`);
      }
      return list[0](args, ctx);
    },

    async checkResource(request, ctx) {
      for (const h of resourceHandlers) {
        const result = await h(request, ctx);
        if ("decline" in result && result.decline) return result;
      }
      return { proceed: true };
    },

    dispatchRender(type: string, event: Record<string, unknown> & { type: string }): void {
      const list = renderHandlers.get(type) ?? [];
      for (const h of list) {
        try {
          h(event);
        } catch {
          /* ignore */
        }
      }
      // Also dispatch to wildcard handler
      const wild = renderHandlers.get("*") ?? [];
      for (const h of wild) {
        try {
          h(event);
        } catch {
          /* ignore */
        }
      }
    },

    toolNames(): string[] {
      return [...toolHandlers.keys()];
    },
  };
}
