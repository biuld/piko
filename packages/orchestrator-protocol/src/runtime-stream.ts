import type {
  AssistantMessage,
  ImageContent,
  Message,
  TextContent,
  ThinkingContent,
  ToolCall,
  Usage,
} from "./messages.js";

export type RuntimeMessageRole = "user" | "assistant" | "toolResult" | "custom";

export interface RuntimeTextBlock {
  type: "text";
  text: string;
}

export interface RuntimeThinkingBlock {
  type: "thinking";
  thinking: string;
  thinkingSignature?: string;
}

export interface RuntimeToolCallBlock {
  type: "toolCall";
  id: string;
  name: string;
  arguments: unknown;
  partialJson?: string;
}

export type RuntimeUserContentBlock = RuntimeTextBlock | ImageContent;

export type RuntimeAssistantContentBlock =
  | RuntimeTextBlock
  | RuntimeThinkingBlock
  | RuntimeToolCallBlock;

export interface RuntimeMessageBase {
  id: string;
  role: RuntimeMessageRole;
  timestamp?: number;
}

export interface RuntimeUserMessage extends RuntimeMessageBase {
  role: "user";
  content: RuntimeUserContentBlock[];
}

export interface RuntimeAssistantMessage extends RuntimeMessageBase {
  role: "assistant";
  content: RuntimeAssistantContentBlock[];
  isStreaming?: boolean;
  stopReason?: string;
  errorMessage?: string;
  usage?: Usage;
  provider?: string;
  model?: string;
}

export interface RuntimeToolResultMessage extends RuntimeMessageBase {
  role: "toolResult";
  toolCallId: string;
  toolName?: string;
  content: unknown;
  isError?: boolean;
}

export interface RuntimeCustomMessage extends RuntimeMessageBase {
  role: "custom";
  customType: string;
  content: unknown;
}

export type RuntimeMessage =
  | RuntimeUserMessage
  | RuntimeAssistantMessage
  | RuntimeToolResultMessage
  | RuntimeCustomMessage;

export type RuntimeAssistantMessageEvent =
  | { type: "start" }
  | { type: "text_start"; contentIndex: number }
  | { type: "text_delta"; contentIndex: number; delta: string }
  | { type: "text_end"; contentIndex: number }
  | { type: "thinking_start"; contentIndex: number }
  | { type: "thinking_delta"; contentIndex: number; delta: string }
  | { type: "thinking_end"; contentIndex: number; contentSignature?: string }
  | { type: "toolcall_start"; contentIndex: number; id: string; name: string }
  | { type: "toolcall_delta"; contentIndex: number; delta: string }
  | { type: "toolcall_end"; contentIndex: number }
  | { type: "done" }
  | { type: "error"; message: string };

export type RuntimeRunStatus = "completed" | "max_steps" | "context_overflow" | "aborted" | "error";

export interface RuntimeEventBase {
  runId: string;
  agentId: string;
}

export type HostRuntimeEvent =
  | (RuntimeEventBase & { type: "agent_start" })
  | (RuntimeEventBase & { type: "agent_end"; status: RuntimeRunStatus; totalSteps?: number })
  | (RuntimeEventBase & { type: "turn_start"; turnIndex: number })
  | (RuntimeEventBase & { type: "turn_end"; turnIndex: number })
  | (RuntimeEventBase & { type: "message_start"; message: RuntimeMessage })
  | (RuntimeEventBase & {
      type: "message_update";
      message: RuntimeMessage;
      assistantEvent?: RuntimeAssistantMessageEvent;
    })
  | (RuntimeEventBase & { type: "message_end"; message: RuntimeMessage })
  | (RuntimeEventBase & {
      type: "tool_execution_start";
      toolCallId: string;
      toolName: string;
      args: unknown;
    })
  | (RuntimeEventBase & {
      type: "tool_execution_update";
      toolCallId: string;
      toolName: string;
      args: unknown;
      partialResult: unknown;
    })
  | (RuntimeEventBase & {
      type: "tool_execution_end";
      toolCallId: string;
      toolName: string;
      result: unknown;
      isError: boolean;
    })
  | (RuntimeEventBase & {
      type: "queue_update";
      steerCount: number;
      followUpCount: number;
      nextTurnCount: number;
      steerPreview?: string;
      followUpPreview?: string;
      nextTurnPreview?: string;
    })
  | (RuntimeEventBase & { type: "failure"; error?: string; aborted: boolean });

export function toRuntimeMessage(message: Message, id: string): RuntimeMessage {
  const timestamp = message.timestamp;
  if (message.role === "user") {
    const content: RuntimeUserContentBlock[] = [];
    if (typeof message.content === "string") {
      content.push({ type: "text", text: message.content });
    } else if (Array.isArray(message.content)) {
      for (const item of message.content) {
        if (item.type === "text") {
          content.push({ type: "text", text: item.text });
        } else if (item.type === "image") {
          content.push({
            type: "image",
            data: item.data,
            mimeType: item.mimeType,
          });
        }
      }
    }
    return {
      id,
      role: "user",
      content,
      timestamp,
    };
  } else if (message.role === "assistant") {
    const content: RuntimeAssistantContentBlock[] = [];
    if (Array.isArray(message.content)) {
      for (const item of message.content) {
        if (item.type === "text") {
          content.push({ type: "text", text: item.text });
        } else if (item.type === "thinking") {
          content.push({
            type: "thinking",
            thinking: item.thinking,
            thinkingSignature: item.thinkingSignature,
          });
        } else if (item.type === "toolCall") {
          content.push({
            type: "toolCall",
            id: item.id,
            name: item.name,
            arguments: item.arguments,
          });
        }
      }
    }
    return {
      id,
      role: "assistant",
      content,
      timestamp,
      stopReason: message.stopReason,
      errorMessage: message.errorMessage,
      usage: message.usage,
      provider: message.provider,
      model: message.model,
    };
  } else if (message.role === "toolResult") {
    let rawContent: unknown = message.details;
    if (rawContent === undefined) {
      if (
        Array.isArray(message.content) &&
        message.content.length > 0 &&
        message.content[0].type === "text"
      ) {
        rawContent = message.content[0].text;
      } else {
        rawContent = message.content;
      }
    }
    return {
      id,
      role: "toolResult",
      toolCallId: message.toolCallId,
      toolName: message.toolName,
      content: rawContent,
      isError: message.isError,
      timestamp,
    };
  } else {
    // Custom/fallback
    return {
      id,
      role: "custom",
      customType: "unknown",
      content: message,
      timestamp,
    };
  }
}

export function toMessage(runtimeMessage: RuntimeMessage): Message {
  const timestamp = runtimeMessage.timestamp ?? Date.now();
  if (runtimeMessage.role === "user") {
    const content: (TextContent | ImageContent)[] = [];
    for (const item of runtimeMessage.content) {
      if (item.type === "text") {
        content.push({ type: "text", text: item.text });
      } else if (item.type === "image") {
        content.push({
          type: "image",
          data: item.data,
          mimeType: item.mimeType,
        });
      }
    }
    return {
      role: "user",
      content,
      timestamp,
    } as Message;
  } else if (runtimeMessage.role === "assistant") {
    const content: (TextContent | ThinkingContent | ToolCall)[] = [];
    for (const item of runtimeMessage.content) {
      if (item.type === "text") {
        content.push({ type: "text", text: item.text });
      } else if (item.type === "thinking") {
        content.push({
          type: "thinking",
          thinking: item.thinking,
          thinkingSignature: item.thinkingSignature,
        });
      } else if (item.type === "toolCall") {
        content.push({
          type: "toolCall",
          id: item.id,
          name: item.name,
          arguments: (item.arguments && typeof item.arguments === "object"
            ? item.arguments
            : {}) as Record<string, any>,
        });
      }
    }
    return {
      role: "assistant",
      content,
      api: "openai-completions",
      provider: runtimeMessage.provider ?? "unknown",
      model: runtimeMessage.model ?? "unknown",
      usage: runtimeMessage.usage ?? {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
      stopReason: (runtimeMessage.stopReason ?? "stop") as any,
      errorMessage: runtimeMessage.errorMessage,
      timestamp,
    } as Message;
  } else if (runtimeMessage.role === "toolResult") {
    const text =
      typeof runtimeMessage.content === "string"
        ? runtimeMessage.content
        : JSON.stringify(runtimeMessage.content, null, 2);
    return {
      role: "toolResult",
      toolCallId: runtimeMessage.toolCallId,
      toolName: runtimeMessage.toolName ?? "",
      content: [{ type: "text", text }],
      details: runtimeMessage.content,
      isError: !!runtimeMessage.isError,
      timestamp,
    } as Message;
  } else {
    return {
      role: "user",
      content:
        typeof runtimeMessage.content === "string"
          ? runtimeMessage.content
          : JSON.stringify(runtimeMessage.content),
      timestamp,
    } as Message;
  }
}

export function providerPartialToRuntimeAssistant(
  partial: AssistantMessage,
  id: string,
  isStreaming = true,
): RuntimeAssistantMessage {
  const content: RuntimeAssistantContentBlock[] = [];
  for (const block of partial.content) {
    if (block.type === "text") {
      content.push({ type: "text", text: block.text });
    } else if (block.type === "thinking") {
      content.push({
        type: "thinking",
        thinking: block.thinking,
        thinkingSignature: block.thinkingSignature,
      });
    } else if (block.type === "toolCall") {
      content.push({
        type: "toolCall",
        id: block.id,
        name: block.name,
        arguments: block.arguments,
      });
    }
  }
  return {
    id,
    role: "assistant",
    content,
    isStreaming,
    stopReason: partial.stopReason,
    errorMessage: partial.errorMessage,
    usage: partial.usage,
    provider: partial.provider,
    model: partial.model,
    timestamp: partial.timestamp,
  };
}
