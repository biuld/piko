import { afterEach, describe, expect, test } from "bun:test";
import { testRender } from "@opentui/solid";
import { createComponent } from "solid-js";
import { ApprovalPanel } from "../src/renderer/opentui/ApprovalPanel.js";

const renderers: Array<{ destroy(): void }> = [];

afterEach(() => {
  for (const renderer of renderers.splice(0)) renderer.destroy();
});

describe("ApprovalPanel", () => {
  test("renders the active request and queued count", async () => {
    const setup = await testRender(
      () =>
        createComponent(ApprovalPanel, {
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
      { width: 80, height: 10 },
    );
    renderers.push(setup.renderer);
    await setup.flush();

    const frame = setup.captureCharFrame();
    expect(frame.replaceAll("─", " ")).toContain("Tool Approval");
    expect(frame).toContain("bash");
    expect(frame).toContain("bun run test");
    expect(frame).toContain("2 more queued");
    expect(frame).toContain("Enter accept");
    expect(frame).toContain("Esc decline");
  });
});
