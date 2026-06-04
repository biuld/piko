// ============================================================================
// Editor — single-line input with border, Enter submits.
// Autocomplete state managed locally by EditorAutocompleteController.
// No per-keystroke global store dispatches. No global surface open/close.
// ============================================================================

import type { InputRenderable, KeyEvent } from "@opentui/core";
import { createEffect, createSignal, onCleanup } from "solid-js";
import { useTheme } from "./theme-context.js";
import { CommandAutocomplete } from "./autocomplete/CommandAutocomplete.js";
import { EditorAutocompleteController } from "../../editor/editor-autocomplete-controller.js";
import { createEmptyAutocompleteState } from "../../editor/editor-autocomplete-state.js";
import type { EditorAutocompleteState } from "../../editor/editor-autocomplete-state.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import type { ActionService } from "./action-service.js";
import type { AutocompleteItem } from "../../autocomplete/types.js";

export interface EditorProps {
  actionSvc: ActionService;
  controller: TuiController;
  disabled: boolean;
  unfocused?: boolean;
}

const AUTOCOMPLETE_HEIGHT = 4;
const AUTOCOMPLETE_MAX_VISIBLE = 3;

export function Editor(props: EditorProps) {
  const theme = useTheme();
  const { actionSvc, controller, disabled, unfocused = false } = props;
  let inputRef: InputRenderable | undefined;
  const [draft, setDraft] = createSignal("");

  // ---- Local autocomplete controller (not in store) ----
  // Wrapped in createSignal for lifecycle safety; Solid re-runs component
  // functions only once, but explicit factory ensures clean disposal.
  const [acState, setAcState] = createSignal<EditorAutocompleteState>(
    createEmptyAutocompleteState(),
  );
  const [editorAc] = createSignal(
    new EditorAutocompleteController(
      controller.autocomplete,
      (s) => setAcState(s),
      undefined,
      (input: string): AutocompleteItem[] => {
        // Sync fallback for instant slash suggestions while async provider loads.
        // Only used when this._state.items is empty and loading.
        if (input.trimStart().startsWith("/")) {
          return controller.getAutocomplete(input);
        }
        return [];
      },
    ),
  );
  const ac = () => editorAc();

  // Register controller with TuiController for global Esc guard
  controller.setAutocompleteController(ac());
  onCleanup(() => {
    ac().dispose();
    controller.setAutocompleteController(null);
  });

  const showSlashMenu = () => {
    const text = draft().trimStart();
    return !disabled && (text.startsWith("/") || text.includes("@"));
  };

  // Query autocomplete provider on draft change
  createEffect(() => {
    const text = draft();
    if (showSlashMenu()) {
      ac().query(text, text.length);
    } else {
      ac().cancel();
    }
  });

  // ---- Key handling (local, no store dispatches) ----
  function handleAutocompleteKey(event: KeyEvent): void {
    if (!showSlashMenu()) return;

    // Esc always cancels autocomplete, even when loading or no results
    if (event.name === "escape") {
      event.preventDefault();
      event.stopPropagation();
      ac().cancel();
      return;
    }

    // Remaining keys require visible items
    const items = ac().visibleItems;
    if (items.length === 0) return;

    if (event.name === "up") {
      event.preventDefault();
      event.stopPropagation();
      ac().move(-1);
      return;
    }

    if (event.name === "down") {
      event.preventDefault();
      event.stopPropagation();
      ac().move(1);
      return;
    }

    if (event.name === "tab") {
      event.preventDefault();
      event.stopPropagation();
      const result = ac().accept();
      if (result) {
        setDraft(result.input);
        if (inputRef) inputRef.value = result.input;
      }
      return;
    }
  }

  // ---- Submit ----
  function handleSubmit(value: string | Record<string, never>): void {
    if (disabled) return;
    const text = typeof value === "string" ? value.trim() : "";
    if (!text) return;

    // If autocomplete is active with slash provider, Enter executes the selected command
    if (showSlashMenu() && ac().isSlashProvider() && ac().visibleItems.length > 0) {
      const selected = ac().getSelectedItem();
      if (selected) {
        const cmd = selected.value;
        ac().cancel();
        inputRef?.clear();
        setDraft("");
        controller.executeSlash(cmd);
        return;
      }
    }

    // Slash command routing
    if (text.startsWith("/")) {
      const found = controller.commands.findBySlash(text.split(" ")[0]);
      if (found) {
        const stateSnapshot = controller.store.state();
        const isIdle = stateSnapshot.stream.status !== "running";

        const avail = controller.commands.checkAvailability(found.id, {
          isStreamRunning: !isIdle,
          hasSession: !!stateSnapshot.session.sessionId,
        });

        if (!avail.available) {
          controller.notifications.notify({
            message: avail.reason,
            severity: "warning",
          });
          ac().cancel();
          inputRef?.clear();
          setDraft("");
          return;
        }

        ac().cancel();
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
      ac().cancel();
      inputRef?.clear();
      setDraft("");
      return;
    }

    // Normal submit
    ac().cancel();
    inputRef?.clear();
    setDraft("");
    actionSvc.submitPrompt(text);
  }

  function handleInput(value: string): void {
    setDraft(value);
  }

  // ---- Render ----
  const visibleItems = () => (showSlashMenu() ? ac().visibleItems : []);
  const selectedIndex = () => acState().selectedIndex;

  return (
    <box flexDirection="column">
      {/* Fixed-height autocomplete lane: never resizes or overdraws timeline. */}
      <box height={AUTOCOMPLETE_HEIGHT} flexShrink={0} overflow="hidden">
        {visibleItems().length > 0 && (
          <CommandAutocomplete
            items={visibleItems()}
            query={draft()}
            selectedIndex={selectedIndex()}
            maxVisible={AUTOCOMPLETE_MAX_VISIBLE}
            onSelect={(item) => {
              const result = controller.autocomplete.applyCompletion(
                draft(),
                draft().length,
                item,
                acState().prefix || draft().trimStart(),
              );
              setDraft(result.input);
              if (inputRef) inputRef.value = result.input;
            }}
            onCancel={() => ac().cancel()}
          />
        )}
      </box>

      {/* Input */}
      <box border={["top", "bottom"]} borderColor={theme.color("border.muted")}>
        <input
          ref={(el: InputRenderable) => {
            inputRef = el;
          }}
          focused={!disabled && !unfocused}
          placeholder={disabled ? "Running..." : "/model  /thinking  /resume  /exit"}
          onKeyDown={handleAutocompleteKey}
          onInput={handleInput}
          onSubmit={handleSubmit as any}
        />
      </box>
    </box>
  );
}
