import type {
  AssistantMessage,
  ToolCall,
  Message,
  TextContent,
} from "@earendil-works/pi-ai";

export function buildAssistantMessage(
  model: string,
  api: string,
  provider: string,
  textContent: string,
  toolCalls: ToolCall[],
  usage: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
    totalTokens: number;
    cost: { input: number; output: number; cacheRead: number; cacheWrite: number; total: number };
  },
  stopReason: "stop" | "length" | "toolUse",
  responseId?: string,
  responseModel?: string,
): AssistantMessage {
  const content: (TextContent | ToolCall)[] = [];

  if (textContent) {
    content.push({ type: "text", text: textContent });
  }

  for (const tc of toolCalls) {
    content.push(tc);
  }

  return {
    role: "assistant",
    content,
    api: api as AssistantMessage["api"],
    provider: provider as AssistantMessage["provider"],
    model,
    responseId,
    responseModel,
    usage,
    stopReason,
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
    isError,
    timestamp: Date.now(),
  };
}

export function buildErrorMessage(
  model: string,
  api: string,
  provider: string,
  errorText: string,
): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text: errorText }],
    api: api as AssistantMessage["api"],
    provider: provider as AssistantMessage["provider"],
    model,
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: "error",
    errorMessage: errorText,
    timestamp: Date.now(),
  };
}
