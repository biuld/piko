import type { AssistantMessage, ToolCall } from "@earendil-works/pi-ai";
import type { Message } from "piko-engine-protocol";

export function buildAssistantMessage(
  textContent: string,
  toolCalls: ToolCall[],
): AssistantMessage {
  const content: AssistantMessage["content"] = [];

  if (textContent) {
    content.push({ type: "text", text: textContent });
  }

  for (const tc of toolCalls) {
    content.push(tc);
  }

  return {
    role: "assistant",
    content,
    api: "openai-completions",
    provider: "unknown",
    model: "unknown",
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: "stop",
    timestamp: Date.now(),
  };
}

export function buildToolResultMessage(
  toolCallId: string,
  toolName: string,
  result: unknown,
  isError: boolean,
): Message {
  const text = typeof result === "string" ? result : JSON.stringify(result);
  return {
    role: "toolResult",
    toolCallId,
    toolName,
    content: [{ type: "text", text }],
    details: result,
    isError,
    timestamp: Date.now(),
  };
}

export function buildErrorMessage(errorText: string): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text: errorText }],
    api: "openai-completions",
    provider: "unknown",
    model: "unknown",
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: "error",
    timestamp: Date.now(),
  };
}
