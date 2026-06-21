import { describe, expect, test } from "bun:test";
import type { ExecutionEnv } from "../src/session/exec-env.js";
import { WorkspaceToolProvider } from "../src/tools/workspace-provider.js";

describe("WorkspaceToolProvider", () => {
  test("forwards cancellation to bash execution", async () => {
    let receivedSignal: AbortSignal | undefined;
    const env = {
      exec: async (_command: string, options?: { abortSignal?: AbortSignal }) => {
        receivedSignal = options?.abortSignal;
        return { ok: true, value: { stdout: "", stderr: "", exitCode: 0 } } as const;
      },
    } as unknown as ExecutionEnv;
    const provider = new WorkspaceToolProvider(env);
    const abort = new AbortController();

    await provider.execute(
      { type: "toolCall", id: "call-1", name: "bash", arguments: { command: "true" } },
      { agentId: "main", taskId: "task-1", toolSetIds: [] },
      abort.signal,
    );

    expect(receivedSignal).toBe(abort.signal);
  });
});
