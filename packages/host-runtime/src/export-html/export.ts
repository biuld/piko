/**
 * Export session to self-contained HTML file.
 *
 * Converts session JSONL / messages into a styled HTML file with:
 * - Theme-aware colors (dark/light CSS variables)
 * - Tool call rendering
 * - Message threading
 */

import type { Message } from "piko-orch-protocol";

// ============================================================================
// Types
// ============================================================================

export interface ExportOptions {
  /** Session messages. */
  messages: Message[];
  /** Session title. */
  sessionName?: string;
  /** Theme name (dark/light). */
  theme?: "dark" | "light";
  /** Output HTML string (returned). */
}

// ============================================================================
// Theme colors
// ============================================================================

const DARK_CSS: Record<string, string> = {
  "--bg": "#1a1a2e",
  "--card-bg": "#2a2a3e",
  "--user-bg": "#343541",
  "--assistant-bg": "#2a2a3e",
  "--tool-bg": "#282832",
  "--text": "#d4d4d4",
  "--muted": "#808080",
  "--accent": "#8abeb7",
  "--border": "#3a3a4a",
  "--success": "#b5bd68",
  "--error": "#cc6666",
  "--code-bg": "#1e1e2e",
};

const LIGHT_CSS: Record<string, string> = {
  "--bg": "#ffffff",
  "--card-bg": "#f8fafc",
  "--user-bg": "#eff6ff",
  "--assistant-bg": "#f8fafc",
  "--tool-bg": "#f1f5f9",
  "--text": "#1e293b",
  "--muted": "#64748b",
  "--accent": "#2563eb",
  "--border": "#e2e8f0",
  "--success": "#16a34a",
  "--error": "#dc2626",
  "--code-bg": "#f1f5f9",
};

// ============================================================================
// Helpers
// ============================================================================

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function formatTimestamp(ts?: number): string {
  if (!ts) return "";
  return new Date(ts).toLocaleString();
}

// ============================================================================
// Message renderers
// ============================================================================

function renderUserMessage(msg: Message): string {
  const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
  const ts = formatTimestamp((msg as { timestamp?: number }).timestamp);
  return `
    <div class="message user-message">
      <div class="message-header">
        <span class="role">👤 User</span>
        <span class="timestamp">${ts}</span>
      </div>
      <div class="message-content">${escapeHtml(content)}</div>
    </div>`;
}

function renderAssistantMessage(msg: Message): string {
  const content = typeof msg.content === "string" ? msg.content : "";

  let toolHtml = "";
  const toolCalls = (msg as { toolCalls?: Array<{ name: string; arguments: unknown }> }).toolCalls;
  if (toolCalls) {
    for (const tc of toolCalls) {
      toolHtml += `
        <div class="tool-call">
          <div class="tool-name">🔧 ${escapeHtml(tc.name)}</div>
          <pre class="tool-args">${escapeHtml(JSON.stringify(tc.arguments, null, 2))}</pre>
        </div>`;
    }
  }

  const thinking = (msg as { thinking?: Array<{ thinking: string }> }).thinking;
  let thinkingHtml = "";
  if (thinking) {
    for (const t of thinking) {
      thinkingHtml += `
        <details class="thinking-block">
          <summary>💭 Thinking</summary>
          <div class="thinking-content">${escapeHtml(t.thinking)}</div>
        </details>`;
    }
  }

  const ts = formatTimestamp((msg as { timestamp?: number }).timestamp);
  return `
    <div class="message assistant-message">
      <div class="message-header">
        <span class="role">🤖 Assistant</span>
        <span class="timestamp">${ts}</span>
      </div>
      ${thinkingHtml}
      ${content ? `<div class="message-content">${escapeHtml(content).replace(/\n/g, "<br>")}</div>` : ""}
      ${toolHtml}
    </div>`;
}

function renderToolResultMessage(msg: Message): string {
  const result = (msg as { toolResult?: { name: string; result: unknown; isError?: boolean } })
    .toolResult;
  if (!result) return "";

  const resultStr =
    typeof result.result === "string" ? result.result : JSON.stringify(result.result, null, 2);
  const isError = result.isError ?? false;

  return `
    <div class="message tool-message ${isError ? "tool-error" : "tool-success"}">
      <div class="message-header">
        <span class="role">🔧 ${escapeHtml(result.name)}</span>
      </div>
      <pre class="tool-output">${escapeHtml(resultStr)}</pre>
    </div>`;
}

function renderMessage(msg: Message): string {
  switch (msg.role) {
    case "user":
      return renderUserMessage(msg);
    case "assistant":
      return renderAssistantMessage(msg);
    case "toolResult":
      return renderToolResultMessage(msg);
    default:
      return renderUserMessage(msg);
  }
}

// ============================================================================
// Export
// ============================================================================

function buildCss(theme: "dark" | "light"): string {
  const vars = theme === "light" ? LIGHT_CSS : DARK_CSS;
  const varDefs = Object.entries(vars)
    .map(([k, v]) => `    ${k}: ${v};`)
    .join("\n");

  return `
    :root {
${varDefs}
    }

    * { margin: 0; padding: 0; box-sizing: border-box; }

    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: var(--bg);
      color: var(--text);
      max-width: 900px;
      margin: 0 auto;
      padding: 20px;
    }

    .header {
      padding: 20px 0;
      border-bottom: 1px solid var(--border);
      margin-bottom: 24px;
    }

    .header h1 {
      font-size: 20px;
      color: var(--accent);
    }

    .header .meta {
      color: var(--muted);
      font-size: 13px;
      margin-top: 4px;
    }

    .message {
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 16px;
      margin-bottom: 12px;
    }

    .user-message { background: var(--user-bg); }
    .assistant-message { background: var(--assistant-bg); }
    .tool-message { background: var(--tool-bg); }
    .tool-error { border-color: var(--error); }
    .tool-success { border-color: var(--success); }

    .message-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 8px;
    }

    .role {
      font-weight: 600;
      font-size: 13px;
      color: var(--accent);
    }

    .timestamp {
      font-size: 12px;
      color: var(--muted);
    }

    .message-content {
      line-height: 1.6;
      white-space: pre-wrap;
    }

    .thinking-block {
      margin-bottom: 8px;
    }

    .thinking-block summary {
      color: var(--muted);
      font-size: 13px;
      cursor: pointer;
    }

    .thinking-content {
      padding: 8px 12px;
      background: var(--code-bg);
      border-radius: 4px;
      font-size: 13px;
      color: var(--muted);
      white-space: pre-wrap;
      margin-top: 4px;
    }

    .tool-call {
      margin-top: 8px;
    }

    .tool-name {
      font-weight: 600;
      font-size: 13px;
      color: var(--text);
      margin-bottom: 4px;
    }

    .tool-args, .tool-output {
      background: var(--code-bg);
      padding: 8px 12px;
      border-radius: 4px;
      font-size: 12px;
      font-family: "SF Mono", Menlo, Monaco, monospace;
      white-space: pre-wrap;
      max-height: 300px;
      overflow-y: auto;
    }
  `;
}

export function exportToHtml(options: ExportOptions): string {
  const { messages, sessionName, theme = "dark" } = options;

  const title = escapeHtml(sessionName ?? "Session Export");
  const count = messages.length;
  const now = new Date().toLocaleString();

  const messageHtml = messages.map(renderMessage).join("\n");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${title}</title>
  <style>${buildCss(theme)}</style>
</head>
<body>
  <div class="header">
    <h1>${title}</h1>
    <div class="meta">${count} messages · Exported ${now}</div>
  </div>
  ${messageHtml}
</body>
</html>`;
}
