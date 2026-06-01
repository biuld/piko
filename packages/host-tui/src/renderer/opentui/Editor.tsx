// ============================================================================
// Editor — growing multi-line text input for user prompts
// Starts at 1 line, grows to max 10 lines, then scrolls.
// Enter submits, Shift+Enter inserts newline.
// ============================================================================

import type { KeyEvent, TextareaRenderable } from "@opentui/core";
import { createSignal } from "solid-js";
import { dispatchCommand } from "./command-dispatcher.js";
import type { KeybindingRegistry } from "./keybinding-registry.js";
import type { TuiStore } from "./store.js";
import type { ActionService } from "./action-service.js";

export interface EditorProps {
  actionSvc: ActionService;
  keybindings: KeybindingRegistry;
  store: TuiStore;
  disabled: boolean;
}

/** Custom keybindings: Enter submits, Shift+Enter inserts newline */
const EDITOR_KEYBINDINGS = [
  { action: "submit" as const, key: "enter" },
  { action: "newline" as const, key: "enter", shift: true },
];

export function Editor(props: EditorProps) {
  const { actionSvc, keybindings, store, disabled } = props;
  let textareaRef: TextareaRenderable | undefined;
  const [lineCount, setLineCount] = createSignal(1);

  function handleContentChange(value: unknown): void {
    const text = typeof value === "string" ? value : "";
    actionSvc.dispatch({ type: "user_input_changed", text });
    // Track line count for dynamic height
    const lines = text.split("\n").length;
    setLineCount(Math.max(1, lines));
  }

  function handleKeyDown(key: KeyEvent): void {
    // Ctrl+D — exit when editor is empty and idle
    if (
      (key.name === "d" && key.ctrl) ||
      key.sequence === "\x04"
    ) {
      if (!disabled) {
        const text = textareaRef?.plainText ?? "";
        if (text.trim().length === 0) {
          actionSvc.shutdown();
        }
      }
    }
  }

  function handleSubmit(): void {
    if (disabled) return;
    const text = textareaRef?.plainText ?? "";
    const t = text.trim();

    // Check slash commands first
    if (t.startsWith("/")) {
      const cmd = keybindings.findSlash(t);
      if (cmd) {
        textareaRef?.clear();
        setLineCount(1);
        actionSvc.dispatch({ type: "user_input_changed", text: "" });
        dispatchCommand(cmd.command, actionSvc, store);
        return;
      }
    }

    // Normal submit
    if (!t) return;
    textareaRef?.clear();
    setLineCount(1);
    actionSvc.dispatch({ type: "user_input_changed", text: "" });
    actionSvc.submitPrompt(t);
  }

  // Dynamic height: 1 line minimum, grows up to 10 lines max
  const editorHeight = () => Math.min(Math.max(lineCount(), 1), 10);

  return (
    <textarea
      ref={(el: TextareaRenderable) => {
        textareaRef = el;
      }}
      keyBindings={EDITOR_KEYBINDINGS as any}
      placeholder={disabled ? "Running..." : "Type... (/model /thinking /resume /exit)"}
      minHeight={1}
      maxHeight={10}
      height={editorHeight()}
      width="100%"
      onContentChange={handleContentChange}
      onKeyDown={handleKeyDown}
      onSubmit={handleSubmit}
    />
  );
}
