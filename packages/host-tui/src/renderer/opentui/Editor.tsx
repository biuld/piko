// ============================================================================
// Editor — single-line input with border, Enter submits natively.
// Slash commands route through the keybinding registry.
// ============================================================================

import type { InputRenderable } from "@opentui/core";
import { useTheme } from "./theme-context.js";
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

export function Editor(props: EditorProps) {
  const theme = useTheme();
  const { actionSvc, keybindings, store, disabled } = props;
  let inputRef: InputRenderable | undefined;

  function handleSubmit(value: string | Record<string, never>): void {
    if (disabled) return;
    const text = typeof value === "string" ? value.trim() : "";
    if (!text) return;

    // Slash command routing
    if (text.startsWith("/")) {
      const cmd = keybindings.findSlash(text);
      if (cmd) {
        // Respect requiresIdle
        if (cmd.requiresIdle && actionSvc.getState().stream.status === "running") {
          actionSvc.dispatch({
            type: "extension_status_set",
            key: "command",
            text: `Command unavailable while running: ${cmd.name}`,
          });
          inputRef?.clear();
          return;
        }
        inputRef?.clear();
        dispatchCommand(cmd.command, actionSvc, store);
        return;
      }

      // Unknown slash command — show error, don't submit
      actionSvc.dispatch({
        type: "extension_status_set",
        key: "command",
        text: `Unknown command: ${text}`,
      });
      inputRef?.clear();
      return;
    }

    // Normal submit
    inputRef?.clear();
    actionSvc.submitPrompt(text);
  }

  return (
    <box border borderColor={theme.color("border.muted")}>
    <input
      ref={(el: InputRenderable) => {
        inputRef = el;
      }}
      placeholder={disabled ? "Running..." : "/model  /thinking  /resume  /exit"}
      onSubmit={handleSubmit as any}
    />
    </box>
  );
}
