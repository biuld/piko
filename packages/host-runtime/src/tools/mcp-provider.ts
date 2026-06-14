import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { McpServerConfig } from "../settings/manager.js";

export class McpToolProvider implements ToolProvider {
  readonly id: string;
  readonly source = "mcp" as const;

  private serverName: string;
  private client: Client;

  constructor(serverName: string, client: Client) {
    this.serverName = serverName;
    this.client = client;
    this.id = `mcp:${serverName}`;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    try {
      const response = await this.client.listTools();
      return response.tools.map((t) => ({
        name: t.name,
        description: t.description ?? "",
        inputSchema: t.inputSchema,
        executor: {
          kind: "mcp",
          target: `${this.serverName}:${t.name}`,
        },
      }));
    } catch (err) {
      console.error(`[McpToolProvider:${this.serverName}] Failed to discover tools:`, err);
      return [];
    }
  }

  async execute(call: ToolCall, _context: ToolExecutionContext): Promise<ToolExecResult> {
    try {
      const response = await this.client.callTool({
        name: call.name,
        arguments: call.arguments,
      });
      const ok = !response.isError;
      let value: any = "";

      if (Array.isArray(response.content)) {
        // Collect text contents from response blocks
        const texts = response.content
          .filter((c: any) => c.type === "text")
          .map((c: any) => c.text)
          .join("\n");
        value = texts;

        // If no text content blocks, stringify the entire content array
        if (!value) {
          value = JSON.stringify(response.content);
        }
      } else {
        value = String(response.content ?? "");
      }

      if (ok) {
        return { ok: true, value };
      } else {
        return {
          ok: false,
          error: {
            code: "mcp_error",
            message: typeof value === "string" ? value : JSON.stringify(value),
          },
        };
      }
    } catch (err) {
      return {
        ok: false,
        error: {
          code: "execution_error",
          message: err instanceof Error ? err.message : String(err),
        },
      };
    }
  }
}

export class McpServerManager {
  private clients = new Map<string, Client>();
  private transports = new Map<string, StdioClientTransport>();
  private providers = new Map<string, McpToolProvider>();

  constructor(private mcpConfigs: Record<string, McpServerConfig> = {}) {}

  async start(): Promise<void> {
    for (const [name, config] of Object.entries(this.mcpConfigs)) {
      try {
        const env: Record<string, string> = {};
        for (const [key, value] of Object.entries(process.env)) {
          if (value !== undefined) {
            env[key] = value;
          }
        }
        if (config.env) {
          for (const [key, value] of Object.entries(config.env)) {
            if (value !== undefined) {
              env[key] = value;
            }
          }
        }

        const transport = new StdioClientTransport({
          command: config.command,
          args: config.args,
          env,
        });

        const client = new Client({ name: "piko-host", version: "0.1.0" }, { capabilities: {} });

        await client.connect(transport);

        this.clients.set(name, client);
        this.transports.set(name, transport);
        this.providers.set(name, new McpToolProvider(name, client));
      } catch (err) {
        console.error(`Failed to start MCP server "${name}":`, err);
      }
    }
  }

  getProviders(): McpToolProvider[] {
    return Array.from(this.providers.values());
  }

  async destroy(): Promise<void> {
    for (const client of this.clients.values()) {
      try {
        await client.close();
      } catch {
        // ignore
      }
    }
    this.clients.clear();
    this.transports.clear();
    this.providers.clear();
  }
}
