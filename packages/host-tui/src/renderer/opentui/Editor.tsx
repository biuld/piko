// ============================================================================
// Editor — multi-line text input for user prompts
// Uses OpenTUI textarea with onContentChange for value tracking
// ============================================================================

import type { TextareaRenderable } from "@opentui/core";
import type { ActionService } from "./action-service.js";

export interface EditorProps {
  actionSvc: ActionService;
  disabled: boolean;
}

export function Editor(props: EditorProps) {
  const { actionSvc, disabled } = props;
  let textareaRef: TextareaRenderable | undefined;

  function handleContentChange(value: unknown): void {
    const text = typeof value === "string" ? value : "";
    actionSvc.dispatch({ type: "user_input_changed", text });
  }

  function handleSubmit(): void {
    if (disabled) return;

    const text = textareaRef?.plainText ?? "";
    const trimmed = text.trim();
    if (!trimmed) return;

    textareaRef?.clear();
    actionSvc.dispatch({ type: "user_input_changed", text: "" });

    actionSvc.submitPrompt(trimmed);
  }

  // Handle slash commands for overlays
  function handleSlashCommand(): void {
    if (disabled) return;
    const text = textareaRef?.plainText ?? "";
    const t = text.trim();

    if (t === "/model") {
      textareaRef?.clear();
      actionSvc.dispatch({
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      });
      return;
    }
    if (t === "/thinking") {
      textareaRef?.clear();
      actionSvc.dispatch({
        type: "overlay_opened",
        overlay: { kind: "thinking", isOpen: true, placement: "modal" },
      });
      return;
    }
    if (t === "/resume") {
      textareaRef?.clear();
      actionSvc.dispatch({
        type: "overlay_opened",
        overlay: { kind: "resume", isOpen: true, placement: "modal" },
      });
      return;
    }
    if (t === "/settings") {
      textareaRef?.clear();
      actionSvc.dispatch({
        type: "overlay_opened",
        overlay: { kind: "settings", isOpen: true, placement: "modal" },
      });
      return;
    }
    if (t === "/login") {
      textareaRef?.clear();
      actionSvc.dispatch({
        type: "overlay_opened",
        overlay: { kind: "login", isOpen: true, placement: "modal" },
      });
      return;
    }
    if (t === "/exit" || t === "/quit") {
      actionSvc.shutdown();
      return;
    }

    // Not a slash command — submit normally
    handleSubmit();
  }

  return (
    <textarea
      ref={(el: TextareaRenderable) => {
        textareaRef = el;
      }}
      placeholder="Type your message... (Enter to send, /model /resume /exit)"
      height="100%"
      width="100%"
      onContentChange={handleContentChange}
      onSubmit={handleSlashCommand}
    />
  );
}
