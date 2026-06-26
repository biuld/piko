import { afterAll, beforeEach, describe, expect, it } from "bun:test";
import { createHostConfig, PikoHost, SessionManager } from "../src/index.js";
import type { Model } from "../src/orchd/protocol/index.js";
import { fs, join, tmpdir } from "./bun-test-utils.js";
import { assistantContent, assistantText, FakeOrchd } from "./helpers/fake-orchd.js";

const PROVIDER = "openai";
const API = "openai-completions";
const MODEL_ID = "host-test-model";

const originalHome = process.env.HOME;

beforeEach(async () => {
  process.env.HOME = await fs.mkdtemp(join(tmpdir(), "piko-host-test-home-"));
});

afterAll(() => {
  process.env.HOME = originalHome;
});

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Host Test Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

describe("PikoHost", () => {
  it("should run a simple prompt and return assistant response", async () => {
    const orchestrator = new FakeOrchd([assistantText("Hello! How can I help?")]);

    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel()),
      orchestrator,
    });

    const result = await host.run("Hi there");

    expect(result.status).toBe("completed");
    expect(result.messages.length).toBeGreaterThanOrEqual(2);

    const userMsg = result.messages.find((m) => m.role === "user");
    expect(userMsg).toBeDefined();

    const assistantMsgs = result.messages.filter((m) => m.role === "assistant");
    expect(assistantMsgs.length).toBeGreaterThan(0);
  });

  it("should handle tool calls", async () => {
    const orchestrator = new FakeOrchd([assistantText("Done")]);

    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel()),
      orchestrator,
    });

    const result = await host.run("Echo hello");
    expect(result.status).toBe("completed");
    expect(
      result.messages.some(
        (msg) =>
          msg.role === "assistant" &&
          Array.isArray(msg.content) &&
          msg.content.some((part) => part.type === "text" && part.text === "Done"),
      ),
    ).toBe(true);
  });

  it("should persist and resume transcript through SessionManager", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-cwd-"));

    const sessionManager = await SessionManager.create(cwd);
    const config = createHostConfig(buildTestModel(), undefined, {
      allowToolCalls: false,
    });
    const orchestrator = new FakeOrchd([
      assistantText("First reply"),
      assistantText("Second reply"),
    ]);

    const host = PikoHost.fromSessionManager(config, sessionManager, { orchestrator });
    const first = await host.run("First prompt");
    expect(first.messages.filter((m) => m.role === "user")).toHaveLength(1);
    expect(first.messages.filter((m) => m.role === "assistant")).toHaveLength(1);
    expect(first.sessionFile).toBeDefined();

    const reopened = await SessionManager.open(first.sessionId, cwd);
    expect(reopened).not.toBeNull();

    const resumedHost = PikoHost.fromSessionManager(config, reopened!, { orchestrator });
    const second = await resumedHost.run("Second prompt");

    expect(second.messages.filter((m) => m.role === "user")).toHaveLength(2);
    expect(second.messages.filter((m) => m.role === "assistant")).toHaveLength(2);

    const finalMessages = await reopened!.loadMessages();
    expect(finalMessages).toHaveLength(4);
  });

  it("should resubmit a selected user entry as a new branch", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-tree-retry-cwd-"));

    const sessionManager = await SessionManager.create(cwd);
    const orchestrator = new FakeOrchd([
      assistantText("First reply"),
      assistantText("Second reply"),
    ]);
    const host = PikoHost.fromSessionManager(
      createHostConfig(buildTestModel(), undefined, { allowToolCalls: false }),
      sessionManager,
      { orchestrator },
    );

    await host.run("Retry this prompt");
    const userEntry = (await host.getTreeEntries()).find(
      (entry) => entry.type === "message" && entry.message.role === "user",
    );
    expect(userEntry).toBeDefined();

    await host.navigateToEntry(userEntry!.id);
    const result = await host.run("Retry this prompt");

    expect(result.messages.filter((message) => message.role === "user")).toHaveLength(1);
    const entries = await host.getTreeEntries();
    expect(
      entries.filter((entry) => entry.type === "message" && entry.message.role === "user"),
    ).toHaveLength(2);
    expect(
      entries.filter((entry) => entry.type === "message" && entry.message.role === "assistant"),
    ).toHaveLength(2);
  });

  it("should expose session management through the host facade", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-facade-cwd-"));

    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: false,
      }),
      session: { cwd },
      orchestrator: new FakeOrchd([assistantText("Facade reply")]),
    });

    await host.run("Name this");
    await host.setSessionName("Named Session");

    expect(await host.getSessionName()).toBe("Named Session");
    expect(host.isSessionPersisted()).toBe(true);

    const listed = await host.listSessions();
    expect(listed).toHaveLength(1);
    expect(listed[0]?.name).toBe("Named Session");

    const renamed = await host.renameSession(host.sessionId, "Renamed Session");
    expect(renamed).toBe(true);
    await host.switchSession(host.sessionId);
    expect(await host.getSessionName()).toBe("Renamed Session");

    const branchEntries = await host.getBranchEntries();
    const userEntry = branchEntries.find(
      (entry) =>
        entry.type === "message" &&
        entry.message.role === "user" &&
        entry.message.content === "Name this",
    );
    expect(userEntry).toBeDefined();

    await host.branchToEntry(userEntry!.id);
    expect(host.getLeafId()).toBe(userEntry!.id);

    const deleted = await host.deleteSession(listed[0]!.id);
    expect(deleted).toBe(true);
  });

  it("should persist pi-style assistant metadata and thinking blocks", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-metadata-cwd-"));

    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: false,
      }),
      session: { cwd },
      orchestrator: new FakeOrchd([
        assistantContent([
          { type: "thinking", thinking: "Reason privately" },
          { type: "text", text: "Final answer" },
        ]),
      ]),
    });

    const result = await host.run("Explain");
    const assistant = result.messages.find((m) => m.role === "assistant");

    expect(assistant).toBeDefined();
    if (assistant?.role === "assistant") {
      expect(assistant.api).toBeDefined();
      expect(assistant.provider).toBeDefined();
      expect(assistant.model).toBeDefined();
      expect(assistant.usage).toBeDefined();
    }
  });

  it("should isolate steering, followUp and nextTurn queues between different agents", async () => {
    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel()),
      orchestrator: new FakeOrchd(),
    });

    // Pushing steering/followUp via public methods throws when idle
    expect(() => host.steer("Main steering", undefined, "main")).toThrow("Cannot steer while idle");
    expect(() => host.followUp("Main followUp", undefined, "main")).toThrow(
      "Cannot follow up while idle",
    );

    host.nextTurn("Main nextTurn", undefined, "main");
    host.nextTurn("Sub nextTurn", undefined, "sub-1");

    // Verify main queue state
    const mainQueue = host.getQueueState("main");
    expect(mainQueue.steering).toHaveLength(0);
    expect(mainQueue.followUp).toHaveLength(0);
    expect(mainQueue.nextTurn).toHaveLength(1);
    expect(mainQueue.nextTurn[0].text).toBe("Main nextTurn");

    // Verify sub queue state
    const subQueue = host.getQueueState("sub-1");
    expect(subQueue.steering).toHaveLength(0);
    expect(subQueue.followUp).toHaveLength(0);
    expect(subQueue.nextTurn).toHaveLength(1);
    expect(subQueue.nextTurn[0].text).toBe("Sub nextTurn");

    // Dequeue main and check that sub remains untouched
    const mainDrained = host.dequeue("main");
    expect(mainDrained.nextTurn).toHaveLength(1);
    expect(mainDrained.nextTurn[0].text).toBe("Main nextTurn");

    expect(host.getQueueState("main").nextTurn).toHaveLength(0);
    expect(host.getQueueState("sub-1").nextTurn).toHaveLength(1);
    expect(host.getQueueState("sub-1").nextTurn[0].text).toBe("Sub nextTurn");
  });

  it("should support running prompts on non-main agents", async () => {
    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel()),
      orchestrator: new FakeOrchd([assistantText("Sub-agent reply")]),
    });

    const result = await host.run("Hello", undefined, "sub-1");
    expect(result.status).toBe("completed");
    expect(result.messages.some((m) => m.role === "assistant")).toBe(true);

    const agentSessions = await host.sessionManager.loadAgentSessions();
    const subSession = agentSessions.find((record) => record.agentId === "sub-1");
    expect(subSession).toBeDefined();

    const tasks = await host.sessionManager.loadTaskTree();
    const subTask = tasks.find((task) => task.agentId === "sub-1");
    expect(subTask).toBeDefined();
    expect(subTask?.agentSessionId).toBe(subSession?.agentSessionId);

    const transcript = await host.sessionManager.loadTaskTranscript(subTask!.taskId);
    expect(
      transcript.some(
        (m) => m.role === "assistant" && JSON.stringify(m.content).includes("Sub-agent reply"),
      ),
    ).toBe(true);
  });

  it("should persist delegated subagent transcripts in attached agent sessions", async () => {
    const orchestrator = new FakeOrchd([assistantText("Review says ok")]);

    const host = await PikoHost.create({
      config: createHostConfig(buildTestModel()),
      orchestrator,
    });

    host.orchestrator!.registerAgent({
      id: "reviewer",
      name: "Reviewer",
      role: "Review implementation",
      systemPrompt: "You review implementation work.",
      toolSetIds: ["builtin"],
    });

    const result = await host.run("Delegate review", undefined, "reviewer");
    expect(result.status).toBe("completed");

    const tasks = await host.sessionManager.loadTaskTree();
    const reviewTask = tasks.find((task) => task.agentId === "reviewer");
    expect(reviewTask).toBeDefined();
    expect(reviewTask?.status).toBe("completed");

    const transcript = await host.sessionManager.loadTaskTranscript(reviewTask!.taskId);
    expect(
      transcript.some(
        (m) => m.role === "assistant" && JSON.stringify(m.content).includes("Review says ok"),
      ),
    ).toBe(true);

    const agentSessions = await host.sessionManager.loadAgentSessions();
    expect(
      agentSessions.some(
        (record) =>
          record.agentId === "reviewer" && record.agentSessionId === reviewTask?.agentSessionId,
      ),
    ).toBe(true);

    const overview = await host.loadSessionPersistenceOverview();
    expect(overview.subagentCount).toBe(1);
    expect(overview.taskCount).toBeGreaterThanOrEqual(1);

    expect(overview.tasks.some((task) => task.agentId === "reviewer")).toBe(true);
  });
});
