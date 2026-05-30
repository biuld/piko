import { afterEach, describe, expect, it } from "vitest";
import * as fs from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Message } from "piko-engine-protocol";
import { PikoSessionRuntime, SessionManager } from "../src/index.js";

const originalHome = process.env.HOME;

afterEach(() => {
  process.env.HOME = originalHome;
});

describe("PikoSessionRuntime", () => {
  it("can replace sessions for new, resume, fork, and clone flows", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-runtime-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-runtime-cwd-"));

    const manager = await SessionManager.create(cwd);
    const messages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi there" }],
        timestamp: Date.now() + 1,
      },
      { role: "user", content: "Fork me", timestamp: Date.now() + 2 },
    ];
    await manager.saveMessages("test-model", messages);

    const runtime = await PikoSessionRuntime.create({ cwd, session: manager.getSessionId() });
    const replaced: string[] = [];
    runtime.setOnSessionReplaced((event) => {
      replaced.push(`${event.reason}:${event.previousSessionId}->${event.nextSessionId}`);
    });

    const clone = await runtime.cloneSession();
    expect(runtime.getSessionManager().getSessionId()).toBe(clone.getSessionId());

    const newSession = await runtime.newSession();
    expect(runtime.getSessionManager().getSessionId()).toBe(newSession.getSessionId());

    const resumed = await runtime.switchSession(manager.getSessionId());
    expect(resumed?.getSessionId()).toBe(manager.getSessionId());
    expect(runtime.getSessionManager().getSessionId()).toBe(manager.getSessionId());

    const entry = (await runtime.getSessionManager().getEntries()).find((item) =>
      item.type === "message" && item.message.role === "user" && item.message.content === "Fork me"
    );
    expect(entry).toBeDefined();

    const forked = await runtime.forkSession(entry!.id);
    expect(runtime.getSessionManager().getSessionId()).toBe(forked.sessionManager.getSessionId());
    expect(forked.selectedText).toBe("Fork me");

    expect(replaced).toHaveLength(4);
    expect(replaced[0]?.startsWith("clone:")).toBe(true);
    expect(replaced[1]?.startsWith("new:")).toBe(true);
    expect(replaced[2]?.startsWith("resume:")).toBe(true);
    expect(replaced[3]?.startsWith("fork:")).toBe(true);
  });
});
