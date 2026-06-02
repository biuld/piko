// ============================================================================
// Editor — single-line input with border, Enter submits.
// Slash commands + autocomplete navigation through TuiController / store.
// ============================================================================

import type { InputRenderable } from "@opentui/core";
import { createEffect, createMemo, createSignal } from "solid-js";
import { useTheme } from "./theme-context.js";
import { CommandAutocomplete } from "./autocomplete/CommandAutocomplete.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import type { TuiStore } from "./store.js";
import type { ActionService } from "./action-service.js";

export interface EditorProps {
  actionSvc: ActionService;
  controller: TuiController;
  store: TuiStore;
  disabled: boolean;
  unfocused?: boolean;
}

export function Editor(props: EditorProps) {
  const theme = useTheme();
  const { actionSvc, controller, store, disabled, unfocused = false } = props;
  let inputRef: InputRenderable | undefined;
  const [draft, setDraft] = createSignal("");

  const state = store.state;

  const showSlashMenu = () => {
    const text = draft().trimStart();
    return !disabled && text.startsWith("/");
  };

  const autocompleteItems = createMemo(() => {
    if (!showSlashMenu()) return [];
    return controller.getAutocomplete(draft());
  });

  const selectedIndex = () => state().autocomplete?.selectedIndex ?? 0;

  // Activate/deactivate autocomplete when slash menu visibility changes
  createEffect(() => {
    if (showSlashMenu() && autocompleteItems().length > 0) {
      if (!state().autocomplete?.active) {
        store.dispatch({ type: "autocomplete_active", active: true, selectedIndex: 0 });
      }
    } else {
      if (state().autocomplete?.active) {
        store.dispatch({ type: "autocomplete_active", active: false });
      }
    }
  });

  // Handle Tab acceptance of autocomplete
  createEffect(() => {
    const token = state().autocomplete?.acceptToken ?? 0;
    if (token > 0 && state().autocomplete?.active) {
      const items = autocompleteItems();
      const idx = Math.min(state().autocomplete!.selectedIndex, items.length - 1);
      const selected = items[idx];
      if (selected) {
        const cmd = selected.value;
        const newText = cmd + " ";
        setDraft(newText);
        if (inputRef) inputRef.value = newText;
        store.dispatch({ type: "user_input_changed", text: newText });
      }
    }
  });

  function handleSubmit(value: string | Record<string, never>): void {
    if (disabled) return;
    const text = typeof value === "string" ? value.trim() : "";
    if (!text) return;

    const autocomplete = state().autocomplete;

    // If autocomplete is active and Enter is pressed, accept selected item AND execute the slash command
    if (autocomplete?.active) {
      const items = autocompleteItems();
      const idx = Math.min(autocomplete.selectedIndex, items.length - 1);
      const selected = items[idx];
      if (selected) {
        const cmd = selected.value;
        store.dispatch({ type: "autocomplete_active", active: false });
        inputRef?.clear();
        setDraft("");
        // Execute the slash command
        controller.executeSlash(cmd);
        return;
      }
    }

    // Slash command routing
    if (text.startsWith("/")) {
      const found = controller.commands.findBySlash(text.split(" ")[0]);
      if (found) {
        const isIdle = state().stream.status !== "running";

        const avail = controller.commands.checkAvailability(found.id, {
          isStreamRunning: !isIdle,
          hasSession: !!state().session.sessionId,
        });

        if (!avail.available) {
          controller.notifications.notify({
            message: avail.reason,
            severity: "warning",
          });
          store.dispatch({ type: "autocomplete_active", active: false });
          inputRef?.clear();
          setDraft("");
          return;
        }

        store.dispatch({ type: "autocomplete_active", active: false });
        inputRef?.clear();
        setDraft("");
        controller.executeSlash(text);
        return;
      }

      // Unknown slash command
      controller.notifications.notify({
        message: `Unknown command: ${text}`,
        severity: "error",
      });
      store.dispatch({ type: "autocomplete_active", active: false });
      inputRef?.clear();
      setDraft("");
      return;
    }

    // Normal submit
    store.dispatch({ type: "autocomplete_active", active: false });
    inputRef?.clear();
    setDraft("");
    actionSvc.submitPrompt(text);
  }

  function handleInput(value: string): void {
    setDraft(value);
    store.dispatch({ type: "user_input_changed", text: value });
  }

  return (
    <box flexDirection="column" live>
      {/* Autocomplete menu */}
      {showSlashMenu() && autocompleteItems().length > 0 && (
        <CommandAutocomplete
          items={autocompleteItems()}
          query={draft()}
          selectedIndex={selectedIndex()}
          onSelect={(item) => {
            const cmd = item.value;
            const newText = cmd + " ";
            setDraft(newText);
            if (inputRef) inputRef.value = newText;
            store.dispatch({ type: "user_input_changed", text: newText });
            store.dispatch({ type: "autocomplete_active", active: false });
          }}
          onCancel={() => {
            store.dispatch({ type: "autocomplete_active", active: false });
          }}
        />
      )}

      {/* Input */}
      <box border={["top", "bottom"]} borderColor={theme.color("border.muted")}>
        <input
          ref={(el: InputRenderable) => {
            inputRef = el;
          }}
          focused={!disabled && !unfocused}
          placeholder={disabled ? "Running..." : "/model  /thinking  /resume  /exit"}
          onInput={handleInput}
          onSubmit={handleSubmit as any}
        />
      </box>
    </box>
  );
}
