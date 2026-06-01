// ============================================================================
// Editor — multi-line text input for user prompts
// Uses OpenTUI textarea with onContentChange for value tracking
// ============================================================================

import type { TextareaRenderable } from "@opentui/core";
import { submitPrompt } from "../../state/actions.js";
import type { ActionContext } from "../../state/actions.js";
import type { TuiStore } from "./store.js";

export interface EditorProps {
  store: TuiStore;
  actionCtx: ActionContext;
  disabled: boolean;
}

export function Editor(props: EditorProps) {
  const { store, actionCtx, disabled } = props;
  let textareaRef: TextareaRenderable | undefined;

  function handleContentChange(value: unknown): void {
    const text = typeof value === "string" ? value : "";
    store.dispatch({ type: "user_input_changed", text });
  }

  function handleSubmit(): void {
    if (disabled) return;

    // Read text directly from the textarea renderable
    const text = textareaRef?.plainText ?? "";
    const trimmed = text.trim();
    if (!trimmed) return;

    // Clear the textarea via the renderable
    textareaRef?.clear();
    store.dispatch({ type: "user_input_changed", text: "" });

    submitPrompt(actionCtx, trimmed);
  }

  return (
    <textarea
      ref={(el: TextareaRenderable) => {
        textareaRef = el;
      }}
      placeholder="Type your message... (Enter to send)"
      height="100%"
      width="100%"
      onContentChange={handleContentChange}
      onSubmit={handleSubmit}
    />
  );
}
