// ============================================================================
// Editor — multi-line text input for user prompts
// Uses slash command registry for command dispatch
// ============================================================================

import type { TextareaRenderable } from "@opentui/core";
import type { ActionService } from "./action-service.js";
import { dispatchCommand } from "./command-dispatcher.js";
import type { KeybindingRegistry } from "./keybinding-registry.js";
import type { TuiStore } from "./store.js";

export interface EditorProps {
  actionSvc: ActionService;
  keybindings: KeybindingRegistry;
  store: TuiStore;
  disabled: boolean;
}

export function Editor(props: EditorProps) {
  const { actionSvc, keybindings, store, disabled } = props;
  let textareaRef: TextareaRenderable | undefined;

  function handleContentChange(value: unknown): void {
    const text = typeof value === "string" ? value : "";
    actionSvc.dispatch({ type: "user_input_changed", text });
  }

  function handleEnter(): void {
    if (disabled) return;
    const text = textareaRef?.plainText ?? "";
    const t = text.trim();

    // Check slash commands first
    if (t.startsWith("/")) {
      const cmd = keybindings.findSlash(t);
      if (cmd) {
        textareaRef?.clear();
        actionSvc.dispatch({ type: "user_input_changed", text: "" });
        dispatchCommand(cmd.command, actionSvc, store);
        return;
      }
    }

    // Normal submit
    if (!t) return;
    textareaRef?.clear();
    actionSvc.dispatch({ type: "user_input_changed", text: "" });
    actionSvc.submitPrompt(t);
  }

  return (
    <textarea
      ref={(el: TextareaRenderable) => {
        textareaRef = el;
      }}
      placeholder="Type... (/model /thinking /resume /exit)"
      height="100%"
      width="100%"
      onContentChange={handleContentChange}
      onSubmit={handleEnter}
    />
  );
}

