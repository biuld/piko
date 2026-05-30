import type {
  Message as PiMessage,
  UserMessage as PiUserMessage,
  AssistantMessage as PiAssistantMessage,
  ToolResultMessage as PiToolResultMessage,
  Model as PiModel,
  Api,
  TextContent as PiTextContent,
  ImageContent as PiImageContent,
  ToolCall as PiToolCall,
} from "@earendil-works/pi-ai";
import type {
  Message,
  UserMessage,
  AssistantMessage,
  ToolResultMessage,
  EngineModel,
  TextContent,
  ImageContent,
  ToolCall,
} from "piko-engine-protocol";

// ---- Protocol → pi-ai ----

export function toPiMessage(msg: Message): PiMessage {
  switch (msg.role) {
    case "user":
      return toPiUserMessage(msg);
    case "assistant":
      return toPiAssistantMessage(msg);
    case "toolResult":
      return toPiToolResultMessage(msg);
  }
}

function toPiUserMessage(msg: UserMessage): PiUserMessage {
  return {
    role: "user",
    content: typeof msg.content === "string"
      ? msg.content
      : msg.content.map(toPiContent),
    timestamp: msg.timestamp,
  };
}

function toPiAssistantMessage(msg: AssistantMessage): PiAssistantMessage {
  return {
    role: "assistant",
    content: msg.content.map(toPiAssistantContent),
    api: "openai-completions" as Api,
    provider: "openai",
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
    timestamp: msg.timestamp,
  };
}

function toPiToolResultMessage(msg: ToolResultMessage): PiToolResultMessage {
  return {
    role: "toolResult",
    toolCallId: msg.toolCallId,
    toolName: msg.toolName,
    content: msg.content.map(toPiContent),
    isError: msg.isError,
    timestamp: msg.timestamp,
  };
}

function toPiContent(block: TextContent | ImageContent): PiTextContent | PiImageContent {
  return block as PiTextContent | PiImageContent;
}

function toPiAssistantContent(
  block: TextContent | ToolCall,
): PiTextContent | PiToolCall {
  if (block.type === "toolCall") {
    return {
      type: "toolCall",
      id: block.id,
      name: block.name,
      arguments: block.arguments,
    };
  }
  return { type: "text", text: block.text };
}

export function toPiModel(model: EngineModel): PiModel<string> {
  return {
    id: model.id,
    name: model.name,
    api: model.api as Api,
    provider: model.provider,
    baseUrl: model.baseUrl,
    reasoning: model.reasoning,
    input: model.input as ("text" | "image")[],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: model.contextWindow,
    maxTokens: model.maxTokens,
  };
}

// ---- pi-ai → Protocol ----

export function fromPiAssistantMessage(
  msg: PiAssistantMessage,
): AssistantMessage {
  return {
    role: "assistant",
    content: msg.content
      .filter((c) => c.type === "text" || c.type === "toolCall")
      .map((c) => {
        if (c.type === "toolCall") {
          return {
            type: "toolCall" as const,
            id: c.id,
            name: c.name,
            arguments: c.arguments,
          };
        }
        return { type: "text" as const, text: c.text };
      }),
    timestamp: msg.timestamp,
  };
}

export function fromPiToolResultMessage(
  msg: PiToolResultMessage,
): ToolResultMessage {
  return {
    role: "toolResult",
    toolCallId: msg.toolCallId,
    toolName: msg.toolName,
    content: msg.content.map((c) => {
      if (c.type === "text") {
        return { type: "text" as const, text: c.text };
      }
      return {
        type: "image" as const,
        data: (c as PiImageContent).data,
        mimeType: (c as PiImageContent).mimeType,
      };
    }),
    isError: msg.isError,
    timestamp: msg.timestamp,
  };
}

export function fromPiUsage(
  usage: PiAssistantMessage["usage"],
): { input: number; output: number; total: number } {
  return {
    input: usage.input,
    output: usage.output,
    total: usage.totalTokens,
  };
}
