import type {
  AssistantMessage,
  ToolCall,
  Message,
  TextContent,
} from "piko-engine-protocol";

export function buildAssistantMessage(
  textContent: string,
  toolCalls: ToolCall[],
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
  errorText: string,
): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text: errorText }],
    timestamp: Date.now(),
  };
}
