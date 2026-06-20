import type { Orchestrator } from "piko-orchestrator";
import type { AgentSpec, ToolProvider } from "piko-orchestrator-protocol";
import { McpServerManager } from "../../tools/mcp-provider.js";
import { builtinToolSet } from "./toolsets.js";

export interface PrepareOrchestratorRunOptions {
  orch: Orchestrator;
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
  agentId,
  systemPrompt,
  activeToolNames,
  mcpServers,
  mcpManager,
}: PrepareOrchestratorRunOptions): Promise<PreparedOrchestratorRun> {
  orch.registerToolSet(builtinToolSet);

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
    toolSetIds: [builtinToolSet.id, ...(mcpToolSetId ? [mcpToolSetId] : [])],
    activeToolNames,
  };

  return { agentSpec, mcpManager: nextMcpManager };
}
