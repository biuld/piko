// ============================================================================
// Session entry → Timeline transcript converter
//
// Converts SessionTreeEntry[] (from piko-session) into TuiMessageViewModel[]
// preserving metadata entries (model_change, thinking_level_change, etc.)
// that buildSessionContext normally filters out.
// ============================================================================

import type { SessionTreeEntry } from "piko-session";
import type { TuiMessageViewModel } from "../state/state.js";

const _msgSeq = 0;

/**
 * Convert a SessionTreeEntry array to TuiMessageViewModel array.
 * Handles all entry types: messages, custom messages, branch summaries,
 * compactions, AND metadata entries (model/thinking/session changes).
 */
export function entriesToTranscript(entries: SessionTreeEntry[]): TuiMessageViewModel[] {
  const result: TuiMessageViewModel[] = [];

  for (const entry of entries) {
    switch (entry.type) {
      case "message": {
        const msg = entry.message;
        const role = mapMessageRole(msg.role);
        if (!role) continue;
        result.push({
          id: entry.id,
          role,
          text: extractText(msg),
        });
        break;
      }

      case "custom_message": {
        result.push({
          id: entry.id,
          role: "custom",
          customType: entry.customType,
          text:
            typeof entry.content === "string"
              ? entry.content
              : Array.isArray(entry.content)
                ? entry.content
                    .filter((c): c is { type: "text"; text: string } => c.type === "text")
                    .map((c) => c.text)
                    .join("\n")
                : "",
        });
        break;
      }

      case "branch_summary": {
        result.push({
          id: entry.id,
          role: "branchSummary",
          text: entry.summary,
        });
        break;
      }

      case "compaction": {
        result.push({
          id: entry.id,
          role: "compactionSummary",
          text: entry.summary,
        });
        break;
      }

      case "model_change":
      case "thinking_level_change":
      case "session_info":
        // These update UI state (status bar, thinking pill, header)
        // rather than appearing as timeline items. Skip them.
        break;

      // Skip these: not visible in timeline
      case "active_tools_change":
      case "custom":
      case "label":
      case "leaf":
        break;
    }
  }

  return result;
}

// ============================================================================
// Helpers
// ============================================================================

function mapMessageRole(role: string): TuiMessageViewModel["role"] | null {
  switch (role) {
    case "user":
      return "user";
    case "assistant":
      return "assistant";
    case "toolResult":
      return "tool";
    default:
      return null;
  }
}

function extractText(msg: { content?: unknown; role?: string }): string {
  if ("content" in msg && msg.content !== undefined) {
    const content = msg.content;
    if (typeof content === "string") return content;
    if (Array.isArray(content)) {
      return content
        .filter((c): c is { type: "text"; text: string } => (c as any).type === "text")
        .map((c: any) => c.text)
        .join("\n");
    }
  }
  return "";
}
