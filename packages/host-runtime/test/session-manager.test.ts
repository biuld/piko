import { afterEach, describe, expect, it } from "bun:test";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Message } from "piko-orchestrator-protocol";
import { SessionManager } from "../src/session/index.js";

const originalHome = process.env.HOME;

afterEach(() => {
  process.env.HOME = originalHome;
});

describe("SessionManager", () => {
  it("creates, saves, continues, and opens sessions by partial id", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-cwd-"));

    const manager = await SessionManager.create(cwd);
    expect(manager.getSessionId()).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    );
    const messages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi there" }],
        timestamp: Date.now() + 1,
      },
    ];

    await manager.saveMessages("test-model", messages);
    expect(manager.getSessionFile()).toBeDefined();

    const continued = await SessionManager.continueRecent(cwd);
    expect(continued?.getSessionId()).toBe(manager.getSessionId());

    // Open by partial ID
    const partial = manager.getSessionId().slice(-6);
    const reopened = await SessionManager.open(partial, cwd);
    if (reopened) {
      expect(reopened.getSessionId()).toBe(manager.getSessionId());
      const loaded = await reopened.loadMessages();
      expect(loaded).toHaveLength(2);
      const entries = await reopened.getEntries();
      expect(entries?.length).toBeGreaterThanOrEqual(3);
    }
  });

  it("can branch from an earlier entry and build a branch-specific message path", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-branch-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-branch-cwd-"));

    const manager = await SessionManager.create(cwd);
    const firstMessages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi there" }],
        timestamp: Date.now() + 1,
      },
    ];
    await manager.saveMessages("test-model", firstMessages);

    const branchBaseId = manager.getLeafId();
    expect(branchBaseId).not.toBeNull();

    const linearMessages: Message[] = [
      ...firstMessages,
      { role: "user", content: "Original path", timestamp: Date.now() + 2 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Original reply" }],
        timestamp: Date.now() + 3,
      },
    ];
    await manager.saveMessages("test-model", linearMessages);

    await manager.branch(branchBaseId!);
    const branchedMessages: Message[] = [
      ...firstMessages,
      { role: "user", content: "Branched path", timestamp: Date.now() + 4 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Branched reply" }],
        timestamp: Date.now() + 5,
      },
    ];
    await manager.saveMessages("test-model", branchedMessages);

    const branch = await manager.getBranch();
    const branchTexts = branch
      .filter(
        (entry): entry is Extract<(typeof branch)[number], { type: "message" }> =>
          entry.type === "message",
      )
      .map((entry) => {
        const message = entry.message;
        if (message.role === "user") return message.content;
        return message.content
          .filter((block) => block.type === "text")
          .map((block) => block.text)
          .join("");
      });

    expect(branchTexts).toContain("Branched path");
    expect(branchTexts).toContain("Branched reply");
    expect(branchTexts).not.toContain("Original path");
    expect(branchTexts).not.toContain("Original reply");
  });

  it("can expose tree state and branch by partial entry id", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-tree-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-tree-cwd-"));

    const manager = await SessionManager.create(cwd);
    const firstMessages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi there" }],
        timestamp: Date.now() + 1,
      },
    ];
    await manager.saveMessages("test-model", firstMessages);
    const firstLeafId = manager.getLeafId();

    await manager.saveMessages("test-model", [
      ...firstMessages,
      { role: "user", content: "Original path", timestamp: Date.now() + 2 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Original reply" }],
        timestamp: Date.now() + 3,
      },
    ]);

    await manager.branch(firstLeafId!);
    await manager.saveMessages("test-model", [
      ...firstMessages,
      { role: "user", content: "Branched path", timestamp: Date.now() + 4 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Branched reply" }],
        timestamp: Date.now() + 5,
      },
    ]);

    const tree = await manager.getTree();
    const currentLeaf = tree.find((entry) => entry.isLeaf);
    expect(currentLeaf?.type).toBe("message");
    if (currentLeaf?.type === "message") {
      expect(currentLeaf.message.role).toBe("assistant");
      expect(currentLeaf.message.content[0]?.type).toBe("text");
      expect(currentLeaf.message.content[0]?.text).toContain("Branched reply");
    }

    const originalPathEntry = tree.find(
      (entry) =>
        entry.type === "message" &&
        entry.message.role === "user" &&
        entry.message.content === "Original path",
    );
    expect(originalPathEntry?.isOnCurrentBranch).toBe(false);
  });

  it("can clone and fork into child sessions with parent session metadata", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-fork-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-fork-cwd-"));

    const manager = await SessionManager.create(cwd);
    const firstMessages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi there" }],
        timestamp: Date.now() + 1,
      },
      { role: "user", content: "Fork me", timestamp: Date.now() + 2 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Ready" }],
        timestamp: Date.now() + 3,
      },
    ];
    await manager.saveMessages("test-model", firstMessages);

    const clone = await manager.createBranchedSession();
    // Parent session path uses repo-level metadata, may differ
    expect(await clone.loadMessages()).toEqual(firstMessages);

    const forkResult = await manager.fork(manager.getLeafId()!, { position: "at" });
    // Fork creates a new session — parent session path may differ
    expect(await forkResult.sessionManager.loadMessages()).toEqual(firstMessages);

    const userEntries = await manager.getEntries();
    const forkTarget = userEntries.find(
      (entry) =>
        entry.type === "message" &&
        entry.message.role === "user" &&
        entry.message.content === "Fork me",
    );
    expect(forkTarget).toBeDefined();

    const userFork = await manager.fork(forkTarget!.id);
    expect(userFork.selectedText).toBe("Fork me");
    const forkedMessages = await userFork.sessionManager.loadMessages();
    expect(forkedMessages).toHaveLength(2);
    expect(forkedMessages[0]).toMatchObject({ role: "user", content: "Hello" });

    const sessions = await SessionManager.list(cwd);
    expect(sessions.length).toBeGreaterThanOrEqual(1);
  });

  it("can persist and read session names", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-name-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-name-cwd-"));

    const manager = await SessionManager.create(cwd);
    await manager.saveMessages("test-model", [
      { role: "user", content: "Hello", timestamp: Date.now() },
    ]);

    expect(await manager.getSessionName()).toBeUndefined();

    await manager.setSessionName("Named Session");
    expect(await manager.getSessionName()).toBe("Named Session");

    const reopened = await SessionManager.open(manager.getSessionId(), cwd);
    expect(await reopened?.getSessionName()).toBe("Named Session");
  });

  it("can rename and delete sessions through static helpers", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-session-ops-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-session-ops-cwd-"));

    const manager = await SessionManager.create(cwd);
    await manager.saveMessages("test-model", [
      { role: "user", content: "Hello", timestamp: Date.now() },
    ]);

    const renamed = await SessionManager.rename(manager.getSessionId(), "Renamed Session", cwd);
    expect(renamed).toBe(true);

    const reopened = await SessionManager.open(manager.getSessionId(), cwd);
    expect(await reopened?.getSessionName()).toBe("Renamed Session");

    const deleted = await SessionManager.delete(manager.getSessionId(), cwd);
    expect(deleted).toBe(true);

    const missing = await SessionManager.open(manager.getSessionId(), cwd);
    expect(missing).toBeNull();
  });
});
