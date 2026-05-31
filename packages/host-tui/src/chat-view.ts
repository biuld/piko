import { type Box, Markdown, Text } from "@earendil-works/pi-tui";
import type { Message } from "piko-engine-protocol";
import { DynamicBorder } from "./components/dynamic-border.js";
import { getMarkdownTheme, getTheme } from "./theme.js";
import { getToolDef, ToolBlock } from "./tools/index.js";

function truncateText(text: string, maxLength = 200): string {
  if (text.length <= maxLength) return text;
  return `${text.slice(0, maxLength - 3)}...`;
}

function stringifyValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export function summarizeToolCall(name: string, args: unknown): string {
  const t = getTheme();
  const suffix = truncateText(stringifyValue(args), 160);
  return `${t.fg("toolTitle", `[tool] ${name} `)}${t.fg("toolOutput", suffix)}`;
}

export function summarizeToolResult(name: string, result: unknown, isError: boolean): string {
  const t = getTheme();
  const prefix = isError
    ? t.fg("error", `[tool error] ${name}`)
    : t.fg("success", `[tool result] ${name}`);
  return `${prefix} ${t.fg("toolOutput", truncateText(stringifyValue(result), 200))}`;
}

export function extractAssistantText(message: Extract<Message, { role: "assistant" }>): string {
  return message.content
    .filter(
      (block): block is Extract<(typeof message.content)[number], { type: "text" }> =>
        block.type === "text",
    )
    .map((block) => block.text)
    .join("\n");
}

// ============================================================================
// Chat view
// ============================================================================

interface ChatMessage {
  role: string;
  text: string;
  /** Reference to a ToolBlock component, if this is a tool message */
  toolBlock?: ToolBlock;
}

export class ChatView {
  private messages: ChatMessage[] = [];
  private chatBox: Box;
  private toolCallIdCounter = 0;

  constructor(chatBox: Box) {
    this.chatBox = chatBox;
  }

  addMessage(role: string, text: string): void {
    this.messages.push({ role, text });
    if (this.messages.length > 100) this.messages.shift();
  }

  /** Create a tool call block. Returns the toolCallId for later result association. */
  startToolCall(name: string, args: unknown, cwd: string): string {
    const toolCallId = `tool-${++this.toolCallIdCounter}`;
    const toolDef = getToolDef(name);
    const block = new ToolBlock(name, toolCallId, args, toolDef, null as any, cwd);
    this.messages.push({ role: "system", text: "", toolBlock: block });
    if (this.messages.length > 100) this.messages.shift();
    return toolCallId;
  }

  /** Update a tool call result */
  endToolCall(_toolCallId: string, result: unknown, isError: boolean): void {
    // Find the ToolBlock by scanning messages for the one with matching toolBlock
    for (const msg of this.messages) {
      if (msg.toolBlock) {
        // We associate by recency — the last pending tool block
      }
    }
    // Simple approach: find most recent tool block and update it
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const msg = this.messages[i];
      if (msg.toolBlock?.isCollapsed) {
        const content = typeof result === "string" ? result : JSON.stringify(result);
        msg.toolBlock.updateResult({ content, isError }, false);
        return;
      }
    }
  }

  /** Replace the last assistant message (for streaming updates) */
  updateLastAssistant(text: string): void {
    for (let i = this.messages.length - 1; i >= 0; i--) {
      if (this.messages[i].role === "assistant") {
        this.messages[i].text = text;
        return;
      }
    }
    this.addMessage("assistant", text);
  }

  rebuildChat(): void {
    const t = getTheme();
    this.chatBox.clear();
    const borderColor = (s: string) => t.fg("borderMuted", s);

    for (const msg of this.messages) {
      if (msg.toolBlock) {
        this.chatBox.addChild(msg.toolBlock);
      } else if (msg.role === "user") {
        this.chatBox.addChild(new DynamicBorder(borderColor));
        this.chatBox.addChild(
          new Markdown(t.fg("userMessageText", `**You:** ${msg.text}`), 1, 0, getMarkdownTheme()),
        );
      } else if (msg.role === "assistant") {
        this.chatBox.addChild(new DynamicBorder(borderColor));
        this.chatBox.addChild(new Markdown(msg.text || "…", 1, 0, getMarkdownTheme()));
      } else if (msg.role === "branchSummary") {
        this.chatBox.addChild(new DynamicBorder(borderColor));
        this.chatBox.addChild(new Text(t.fg("customMessageLabel", "📋 Branch summary"), 1, 0));
        this.chatBox.addChild(new Text(t.fg("customMessageText", msg.text), 1, 0));
      } else if (msg.role === "compactionSummary") {
        this.chatBox.addChild(new DynamicBorder(borderColor));
        this.chatBox.addChild(new Text(t.fg("customMessageLabel", "📦 Compaction"), 1, 0));
        this.chatBox.addChild(new Text(t.fg("customMessageText", msg.text), 1, 0));
      } else {
        this.chatBox.addChild(new Text(t.fg("muted", msg.text), 1, 0));
      }
    }
  }

  rebuildFromTranscript(transcript: Message[], systemMessage?: string): void {
    this.messages.length = 0;
    let lastToolId: string | null = null;
    for (const msg of transcript) {
      if (msg.role === "user") {
        this.addMessage("user", typeof msg.content === "string" ? msg.content : "");
        continue;
      }
      if (msg.role === "assistant") {
        const text = extractAssistantText(msg);
        if (text.trim()) {
          this.addMessage("assistant", text);
        }
        for (const block of msg.content) {
          if (block.type === "toolCall") {
            lastToolId = this.startToolCall(block.name, block.arguments, "");
          }
        }
        continue;
      }
      // Tool result — associate with the preceding tool call
      if (lastToolId) {
        this.endToolCall(lastToolId, msg.details ?? msg.content, msg.isError);
        lastToolId = null;
        continue;
      }
      // Branch summary
      if ((msg as unknown as Record<string, unknown>).role === "branchSummary") {
        const bs = msg as unknown as { role: "branchSummary"; summary: string };
        this.addMessage("branchSummary", bs.summary);
        continue;
      }
      // Compaction summary
      if ((msg as unknown as Record<string, unknown>).role === "compactionSummary") {
        const cs = msg as unknown as {
          role: "compactionSummary";
          summary: string;
          tokensBefore: number;
        };
        this.addMessage(
          "compactionSummary",
          `[${cs.tokensBefore.toLocaleString()} tokens → compacted]\n${cs.summary}`,
        );
        continue;
      }
      // Unknown message — treat as system
      const anyMsg = msg as unknown as {
        toolName?: string;
        role?: string;
        content?: unknown;
        details?: unknown;
        isError?: boolean;
      };
      this.addMessage(
        "system",
        summarizeToolResult(
          anyMsg.toolName ?? anyMsg.role ?? "unknown",
          anyMsg.content ?? anyMsg.details,
          anyMsg.isError ?? false,
        ),
      );
    }
    if (systemMessage) {
      this.addMessage("system", systemMessage);
    }
  }
}
