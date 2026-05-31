import type { Message } from "piko-engine-protocol";
import type { PikoHost, StreamPromptResult } from "piko-host-runtime";

export interface StreamingHandlers {
  onAssistantDelta: (text: string) => void;
  onThinkingDelta: (delta: string) => void;
  onToolCallStart: (name: string, args: unknown, eventId: string) => void;
  onToolCallEnd: (name: string, result: unknown, isError: boolean, eventId: string) => void;
}

export interface StreamingResult {
  text: string;
  messages: Message[];
  status: StreamPromptResult["status"];
}

export async function runStreaming(
  host: PikoHost,
  prompt: string,
  signal: AbortSignal,
  handlers: StreamingHandlers,
): Promise<StreamingResult> {
  const stream = host.streamPrompt(
    prompt,
    {
      settingsOverride: {
        maxSteps: 10,
        allowToolCalls: true,
        allowApprovals: true,
      },
    },
    signal,
  );
  let text = "";
  const toolCallNames = new Map<string, string>();

  for await (const event of stream) {
    if (event.type === "message_delta") {
      text += (event as { delta: string }).delta;
      handlers.onAssistantDelta(text);
    } else if (event.type === "thinking_delta") {
      handlers.onThinkingDelta(event.delta);
    } else if (event.type === "tool_call_start") {
      toolCallNames.set(event.id, event.name);
      handlers.onToolCallStart(event.name, event.args, event.id);
    } else if (event.type === "tool_call_end") {
      const toolName = toolCallNames.get(event.id) ?? event.id;
      handlers.onToolCallEnd(toolName, event.result, event.isError, event.id);
    }
  }

  const result = await stream.result();
  for (const msg of result.appendedMessages) {
    if (msg.role === "assistant") {
      for (const block of msg.content) {
        if (block.type === "text") {
          text = block.text;
        }
      }
    }
  }

  return {
    text: text || "(empty response)",
    messages: (result as StreamPromptResult).messages,
    status: result.status,
  };
}
