import { existsSync } from "node:fs";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Message } from "piko-engine-protocol";
import { afterEach, describe, expect, it } from "vitest";
import {
  appendSessionInfo,
  findMostRecentSession,
  getSessionDir,
  getSessionsDir,
  listAllSessions,
  listSessions,
  loadSession,
  resolveSession,
  saveSession,
} from "../src/session/file-session-store.js";

const originalHome = process.env.HOME;

afterEach(async () => {
  process.env.HOME = originalHome;
});

describe("file-session-store", () => {
  it("writes pi-style jsonl session files under cwd-scoped session directories", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-home-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-cwd-"));

    const messages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi" }],
        timestamp: Date.now() + 1,
      },
    ];

    await saveSession("session-abc123", "test-model", messages, cwd);

    const sessionsDir = getSessionsDir();
    expect(existsSync(sessionsDir)).toBe(true);
    expect(getSessionDir(cwd).startsWith(sessionsDir)).toBe(true);

    const files = await fs.readdir(getSessionDir(cwd));
    expect(files).toHaveLength(1);
    expect(files[0]).toContain("session-abc123");
    expect(files[0]).toMatch(/\.jsonl$/);

    const saved = await fs.readFile(join(getSessionDir(cwd), files[0]!), "utf-8");
    const lines = saved
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(lines[0]?.type).toBe("session");
    expect(lines[0]?.id).toBe("session-abc123");
    expect(lines[0]?.version).toBe(3);
    expect(lines[1]?.type).toBe("model_change");
    expect(lines[1]?.modelId).toBe("test-model");
    expect(lines[1]?.id).toMatch(/^[0-9a-f]{8}$/);
    expect(lines[2]?.type).toBe("message");
    expect(lines[2]?.id).toMatch(/^[0-9a-f]{8}$/);
    expect(lines[3]?.type).toBe("message");

    const loaded = await loadSession("session-abc123", cwd);
    expect(loaded).toHaveLength(2);

    const listed = await listSessions(cwd);
    expect(listed).toHaveLength(1);
    expect(listed[0]?.id).toBe("session-abc123");
    expect(listed[0]?.messageCount).toBe(2);
    expect(listed[0]?.model).toBe("test-model");

    const resolved = await resolveSession("abc123", cwd);
    expect(resolved?.id).toBe("session-abc123");

    const latest = await findMostRecentSession(cwd);
    expect(latest?.id).toBe("session-abc123");
  });

  it("appends only new messages instead of rewriting prior entries", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-home-append-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-cwd-append-"));

    const firstMessages: Message[] = [
      { role: "user", content: "Hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: [{ type: "text", text: "Hi" }],
        timestamp: Date.now() + 1,
      },
    ];

    await saveSession("session-append", "test-model", firstMessages, cwd);

    const secondMessages: Message[] = [
      ...firstMessages,
      { role: "user", content: "Next", timestamp: Date.now() + 2 },
      {
        role: "assistant",
        content: [{ type: "text", text: "Again" }],
        timestamp: Date.now() + 3,
      },
    ];

    await saveSession("session-append", "test-model", secondMessages, cwd);

    const files = await fs.readdir(getSessionDir(cwd));
    const saved = await fs.readFile(join(getSessionDir(cwd), files[0]!), "utf-8");
    const lines = saved
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    const messageLines = lines.filter((entry) => entry.type === "message");
    expect(messageLines).toHaveLength(4);
    expect(messageLines[0]?.message).toMatchObject({ role: "user", content: "Hello" });
    expect(messageLines[2]?.message).toMatchObject({ role: "user", content: "Next" });
  });

  it("stores session_info entries and exposes names in session metadata", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-home-name-"));
    process.env.HOME = home;
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-cwd-name-"));

    await saveSession(
      "session-name",
      "test-model",
      [{ role: "user", content: "Hello", timestamp: Date.now() }],
      cwd,
    );
    await appendSessionInfo("session-name", cwd, "Named Session");

    const files = await fs.readdir(getSessionDir(cwd));
    const saved = await fs.readFile(join(getSessionDir(cwd), files[0]!), "utf-8");
    const lines = saved
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(
      lines.some((entry) => entry.type === "session_info" && entry.name === "Named Session"),
    ).toBe(true);

    const listed = await listSessions(cwd);
    expect(listed[0]?.name).toBe("Named Session");
  });

  it("can list sessions across cwd buckets", async () => {
    const home = await fs.mkdtemp(join(tmpdir(), "piko-home-all-"));
    process.env.HOME = home;
    const cwdA = await fs.mkdtemp(join(tmpdir(), "piko-cwd-a-"));
    const cwdB = await fs.mkdtemp(join(tmpdir(), "piko-cwd-b-"));

    await saveSession(
      "session-a",
      "test-model",
      [{ role: "user", content: "A", timestamp: Date.now() }],
      cwdA,
    );
    await saveSession(
      "session-b",
      "test-model",
      [{ role: "user", content: "B", timestamp: Date.now() + 1 }],
      cwdB,
    );

    const allSessions = await listAllSessions();
    expect(allSessions.map((session) => session.id)).toEqual(
      expect.arrayContaining(["session-a", "session-b"]),
    );
    expect(new Set(allSessions.map((session) => session.cwd))).toEqual(new Set([cwdA, cwdB]));
  });
});
