import type { Orchestrator } from "piko-orchestrator";
import type { AgentSpec, ToolProvider } from "piko-orchestrator-protocol";
import type { HostConfig } from "../../models/index.js";
import { McpServerManager } from "../../tools/mcp-provider.js";
import { builtinToolNames, builtinToolSet } from "./toolsets.js";

export interface PrepareOrchestratorRunOptions {
  orch: Orchestrator;
  config: HostConfig;
  agentId: string;
  systemPrompt: string;
  activeToolNames: string[] | undefined;
  mcpServers?: Record<string, unknown>;
  mcpManager?: McpServerManager;
}

export interface PreparedOrchestratorRun {
  agentSpec: AgentSpec;
  mcpManager?: McpServerManager;
}

export async function prepareOrchestratorRun({
  orch,
  config,
  agentId,
  systemPrompt,
  activeToolNames,
  mcpServers,
  mcpManager,
}: PrepareOrchestratorRunOptions): Promise<PreparedOrchestratorRun> {
  orch.registerToolSet(builtinToolSet);
  const customToolNames = (config.tools ?? [])
    .map((tool) => tool.name)
    .filter((name) => !builtinToolNames.has(name));

  if (customToolNames.length > 0) {
    orch.registerToolSet({
      id: "custom",
      name: "Custom",
      tools: customToolNames.map((toolName) => ({
        kind: "provider_tool",
        providerId: "workspace",
        toolName,
        policy: { sensitivity: "safe", approval: "never" },
      })),
    });
  }

  let mcpToolSetId: string | undefined;
  let nextMcpManager = mcpManager;

  if (mcpServers && Object.keys(mcpServers).length > 0) {
    if (!nextMcpManager) {
      nextMcpManager = new McpServerManager(
        mcpServers as ConstructorParameters<typeof McpServerManager>[0],
      );
      await nextMcpManager.start();

      for (const provider of nextMcpManager.getProviders()) {
        orch.registerProvider(provider as ToolProvider);
      }
    }

    const mcpProviders = nextMcpManager.getProviders();
    if (mcpProviders.length > 0) {
      mcpToolSetId = "mcp";
      orch.registerToolSet({
        id: "mcp",
        name: "MCP Tools",
        tools: mcpProviders.map((provider) => ({
          kind: "provider_namespace",
          providerId: provider.id,
          namespace: "",
          policy: { sensitivity: "sensitive", approval: "on_sensitive" },
        })),
      });
    }
  }

  const agentSpec: AgentSpec = {
    id: agentId,
    name: agentId === "main" ? "Main" : agentId,
    role: "Coding assistant.",
    systemPrompt,
    toolSetIds: [
      builtinToolSet.id,
      ...(customToolNames.length > 0 ? ["custom"] : []),
      ...(mcpToolSetId ? [mcpToolSetId] : []),
    ],
    activeToolNames,
    concurrency: { maxConcurrentTasks: 1 },
  };

  return { agentSpec, mcpManager: nextMcpManager };
}
