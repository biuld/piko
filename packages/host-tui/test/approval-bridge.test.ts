import { describe, expect, test } from "bun:test";
import { createApprovalBridge } from "../src/approval-bridge.js";
import type { ToolApprovalRequest } from "../src/shared/index.js";

const request: ToolApprovalRequest = {
  toolEntityId: "assistant-run:tool:0",
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

  test("delivers parallel approvals in FIFO order", async () => {
    const bridge = createApprovalBridge();
    const decisions = [
      bridge.handler(request),
      bridge.handler({ ...request, callId: "call-2", toolName: "edit" }),
      bridge.handler({ ...request, callId: "call-3", toolName: "write" }),
    ];
    const delivered: string[] = [];
    bridge.onPending((pending) => {
      delivered.push(pending.request.callId);
      pending.resolve(pending.request.callId === "call-2" ? "decline" : "accept");
    });

    expect(delivered).toEqual(["call-1", "call-2", "call-3"]);
    expect(await Promise.all(decisions)).toEqual(["accept", "decline", "accept"]);
  });

  test("declines instead of hanging when the TUI listener throws", async () => {
    const bridge = createApprovalBridge();
    bridge.onPending(() => {
      throw new Error("render failed");
    });

    expect(await bridge.handler(request)).toBe("decline");
  });
});
