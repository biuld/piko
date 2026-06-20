import { afterEach, describe, expect, test } from "bun:test";
import { testRender } from "@opentui/solid";
import { createComponent } from "solid-js";
import { ToolApprovalBody } from "../src/renderer/opentui/panels/ToolApprovalBody.js";

const renderers: Array<{ destroy(): void }> = [];

afterEach(() => {
  for (const renderer of renderers.splice(0)) renderer.destroy();
});

describe("ToolApprovalBody", () => {
  test("renders the active request and queued count", async () => {
    // Build a minimal TuiStore-like object with approval state
    const mockStore = {
      state: () => ({
        approval: {
          pending: {
            callId: "call-1",
            toolName: "bash",
            toolArgs: { command: "bun run test" },
          },
          queue: [
            { callId: "call-2", toolName: "edit", toolArgs: { path: "src/a.ts" } },
            { callId: "call-3", toolName: "write", toolArgs: { path: "src/b.ts" } },
          ],
        },
      }),
    } as any;

    const setup = await testRender(() => createComponent(ToolApprovalBody, { store: mockStore }), {
      width: 80,
      height: 6,
    });
    renderers.push(setup.renderer);
    await setup.flush();

    const frame = setup.captureCharFrame();
    expect(frame).toContain("Permission required");
    expect(frame).toContain("bash");
    expect(frame).toContain("bun run test");
  });
});
