import { describe, expect, mock, test } from "bun:test";

// Mock the MCP SDK modules BEFORE importing the provider/manager
mock.module("@modelcontextprotocol/sdk/client/index.js", () => {
  return {
    Client: class {
      connect = async () => {};
      close = async () => {};
      listTools = async () => ({ tools: [] });
      callTool = async () => ({ content: [] });
    },
  };
});

mock.module("@modelcontextprotocol/sdk/client/stdio.js", () => {
  return {
    StdioClientTransport: class {},
  };
});

// Import after mocking modules
import { McpServerManager, McpToolProvider } from "../src/tools/mcp-provider.js";

describe("McpToolProvider", () => {
  test("discover returns mapped tools", async () => {
    const mockClient = {
      listTools: async () => ({
        tools: [
          {
            name: "test_tool",
            description: "A test tool",
            inputSchema: { type: "object", properties: { input: { type: "string" } } },
          },
        ],
      }),
    } as any;

    const provider = new McpToolProvider("test-server", mockClient);
    const tools = await provider.discover({ agentId: "main", toolSetIds: [] });

    expect(tools).toHaveLength(1);
    expect(tools[0].name).toBe("test_tool");
    expect(tools[0].description).toBe("A test tool");
    expect(tools[0].executor.kind).toBe("mcp");
    expect(tools[0].executor.target).toBe("test-server:test_tool");
  });

  test("execute forwards arguments and maps response", async () => {
    const mockClient = {
      callTool: async (params: { name: string; arguments: any }) => {
        expect(params.name).toBe("test_tool");
        expect(params.arguments).toEqual({ input: "hello" });
        return {
          content: [{ type: "text", text: "result output" }],
          isError: false,
        };
      },
    } as any;

    const provider = new McpToolProvider("test-server", mockClient);
    const result = await provider.execute(
      {
        id: "call-1",
        name: "test_tool",
        arguments: { input: "hello" },
      },
      { agentId: "main", taskId: "task-1", toolSetIds: [] },
    );

    expect(result.ok).toBe(true);
    expect(result.value).toBe("result output");
  });

  test("execute maps error responses", async () => {
    const mockClient = {
      callTool: async () => {
        return {
          content: [{ type: "text", text: "error output" }],
          isError: true,
        };
      },
    } as any;

    const provider = new McpToolProvider("test-server", mockClient);
    const result = await provider.execute(
      {
        id: "call-1",
        name: "test_tool",
        arguments: {},
      },
      { agentId: "main", taskId: "task-1", toolSetIds: [] },
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("mcp_error");
    expect(result.error?.message).toBe("error output");
  });
});

describe("McpServerManager", () => {
  test("starts and registers configured servers", async () => {
    const manager = new McpServerManager({
      server1: { command: "node", args: ["server1.js"] },
      server2: { command: "python", args: ["server2.py"], env: { KEY: "VAL" } },
    });

    await manager.start();

    const providers = manager.getProviders();
    expect(providers).toHaveLength(2);
    // StdioClientTransport connected and registered
    expect(providers[0].id).toBe("mcp:server1");
    expect(providers[1].id).toBe("mcp:server2");

    await manager.destroy();
  });

  test("does not crash on start failure of some servers", async () => {
    // Override the mock to throw on connect
    mock.module("@modelcontextprotocol/sdk/client/index.js", () => {
      return {
        Client: class {
          connect = async () => {
            throw new Error("Failed to connect");
          };
          close = async () => {};
        },
      };
    });

    const manager = new McpServerManager({
      brokenServer: { command: "invalid_command" },
    });

    // Should not throw, should log error and continue
    await manager.start();

    expect(manager.getProviders()).toHaveLength(0);
  });
});
