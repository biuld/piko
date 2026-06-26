import { describe, expect, it } from "bun:test";
import type { Message } from "../src/orchd/protocol/index.js";
import {
  addUserMessage,
  appendMessages,
  createSession,
  updateSessionState,
} from "../src/session/session-store.js";

describe("session-store", () => {
  it("createSession initializes session state correctly with defaults", () => {
    const session = createSession({ systemPrompt: "test system prompt" });
    expect(session.sessionId).toStartWith("session-");
    expect(session.messages).toEqual([]);
    expect(session.systemPrompt).toBe("test system prompt");
    expect(session.createdAt).toBeLessThanOrEqual(Date.now());
    expect(session.updatedAt).toBeLessThanOrEqual(Date.now());
    expect(session.runState).toBe("idle");
    expect(session.engineState).toBeUndefined();
  });

  it("createSession respects overrides", () => {
    const customTime = Date.now() - 10000;
    const session = createSession({
      sessionId: "custom-id",
      messages: [{ role: "user", content: "hello", timestamp: customTime }],
      systemPrompt: "custom prompt",
      createdAt: customTime,
      updatedAt: customTime,
      runState: "running",
      engineState: { step: 1 },
    });

    expect(session.sessionId).toBe("custom-id");
    expect(session.messages).toHaveLength(1);
    expect(session.messages[0].content).toBe("hello");
    expect(session.systemPrompt).toBe("custom prompt");
    expect(session.createdAt).toBe(customTime);
    expect(session.updatedAt).toBe(customTime);
    expect(session.runState).toBe("running");
    expect(session.engineState).toEqual({ step: 1 });
  });

  it("appendMessages adds messages and updates updatedAt timestamp", () => {
    const session = createSession({ systemPrompt: "prompt" });
    const initialUpdatedAt = session.updatedAt;

    const newMsg: Message = {
      role: "assistant",
      content: [{ type: "text" as const, text: "hi" }],
      timestamp: Date.now(),
    } as any;
    const updated = appendMessages(session, [newMsg]);

    expect(updated.messages).toEqual([newMsg]);
    expect(updated.updatedAt).toBeGreaterThanOrEqual(initialUpdatedAt);
  });

  it("updateSessionState updates runState and engineState and updates updatedAt timestamp", () => {
    const session = createSession({ systemPrompt: "prompt" });
    const initialUpdatedAt = session.updatedAt;

    const updated = updateSessionState(session, {
      runState: "completed",
      engineState: { step: 2 },
    });
    expect(updated.runState).toBe("completed");
    expect(updated.engineState).toEqual({ step: 2 });
    expect(updated.updatedAt).toBeGreaterThanOrEqual(initialUpdatedAt);
  });

  it("addUserMessage adds user text message and sets runState to idle", () => {
    const session = createSession({ systemPrompt: "prompt", runState: "completed" });
    const updated = addUserMessage(session, "user input text");

    expect(updated.runState).toBe("idle");
    expect(updated.messages).toHaveLength(1);
    expect(updated.messages[0].role).toBe("user");
    expect(updated.messages[0].content).toBe("user input text");
  });

  it("addUserMessage adds user message with images content", () => {
    const session = createSession({ systemPrompt: "prompt" });
    const imageContent = { type: "image" as const, mimeType: "image/png", data: "base64" };
    const updated = addUserMessage(session, "user input with image", [imageContent]);

    expect(updated.messages).toHaveLength(1);
    expect(updated.messages[0].role).toBe("user");
    expect(updated.messages[0].content).toEqual([
      { type: "text" as const, text: "user input with image" },
      imageContent,
    ]);
  });
});
