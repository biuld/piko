import { describe, expect, test } from "bun:test";
import type { ToolApprovalRequest } from "piko-orchestrator-protocol";
import { createApprovalBridge } from "../src/approval-bridge.js";

const request: ToolApprovalRequest = {
  callId: "call-1",
  agentId: "main",
  taskId: "task-1",
  toolName: "bash",
  toolArgs: {},
};

describe("approval bridge", () => {
  test("delivers an approval that arrives before the TUI listener is registered", async () => {
    const bridge = createApprovalBridge();
    const decision = bridge.handler(request);

    bridge.onPending((pending) => pending.resolve("accept"));

    expect(await decision).toBe("accept");
  });

  test("removes a buffered approval when its run is aborted", async () => {
    const bridge = createApprovalBridge();
    const abort = new AbortController();
    const decision = bridge.handler(request, abort.signal);
    abort.abort();

    let delivered = false;
    bridge.onPending(() => {
      delivered = true;
    });

    expect(await decision).toBe("decline");
    expect(delivered).toBe(false);
  });
});
