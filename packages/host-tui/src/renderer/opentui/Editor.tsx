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
import type { KeyEvent as FocusKeyEvent } from "../../focus/types.js";

export interface EditorProps {
  actionSvc: ActionService;
  controller: TuiController;
  disabled: boolean;
  unfocused?: boolean;
}

const AUTOCOMPLETE_MAX_VISIBLE = 8;
const AUTOCOMPLETE_HEIGHT = AUTOCOMPLETE_MAX_VISIBLE + 1;

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
  // Register autocomplete key handler so TuiController can intercept
  // autocomplete keys before focus routing (avoids focus stack changes)
  controller.setAutocompleteKeyHandler((event: FocusKeyEvent) => handleAutocompleteKey(event));
  onCleanup(() => {
    ac().dispose();
    controller.setAutocompleteController(null);
    controller.setAutocompleteKeyHandler(null);
  });

  const showSlashMenu = () => {
    const text = draft().trimStart();
    return !disabled && (text.startsWith("/") || text.includes("@"));
  };

  const syncSlashItems = (): AutocompleteItem[] => {
    const text = draft();
    if (!text.trimStart().startsWith("/")) return [];
    return controller.getAutocomplete(text);
  };

  const visibleItems = () => {
    if (!showSlashMenu()) return [];
    const state = acState();
    if (state.items.length > 0) return state.items;
    if (state.loading) {
      const fallback = syncSlashItems();
      if (fallback.length > 0) return fallback;
    }
    return state.items;
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

  // ---- Local autocomplete key handler ----
  // Autocomplete is an editor-local interaction: keys are intercepted
  // on the <input> BEFORE reaching the global keyboard handler.
  // This avoids the infinite remount loop that occurs when pushing/popping
  // focus (focus_changed → state() → plan() → <For> remounts Editor).
  function handleAutocompleteKey(event: FocusKeyEvent): boolean {
    if (!autocompleteVisible()) return false;

    if (event.name === "up") {
      ac().move(-1);
      return true;
    }
    if (event.name === "down") {
      ac().move(1);
      return true;
    }
    if (event.name === "tab") {
      const result = ac().accept();
      if (result) {
        setDraft(result.input);
        if (inputRef) inputRef.value = result.input;
      }
      return true;
    }
    if (event.name === "escape") {
      ac().cancel();
      return true;
    }
    if (event.name === "enter" || event.name === "return") {
      const items = visibleItems();
      const slashDraft = draft().trimStart();
      if (items.length > 0 && slashDraft.startsWith("/") && !/\s/.test(slashDraft)) {
        const selected =
          ac().getSelectedItem() ?? items[Math.min(acState().selectedIndex, items.length - 1)];
        if (selected) {
          const cmd = selected.value;
          ac().cancel();
          inputRef?.clear();
          setDraft("");
          controller.executeSlash(cmd);
          return true;
        }
      }
    }
    return false;
  }

  // ---- Submit ----
  function handleSubmit(value: string | Record<string, never>): void {
    if (disabled) return;
    const text = typeof value === "string" ? value.trim() : "";
    if (!text) return;

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
  const selectedIndex = () => acState().selectedIndex;
  const autocompleteVisible = () => visibleItems().length > 0;

  return (
    <box flexDirection="column">
      {/* Editor-local autocomplete only takes space while it is actually visible. */}
      <box height={autocompleteVisible() ? AUTOCOMPLETE_HEIGHT : 0} flexShrink={0} overflow="hidden">
        {autocompleteVisible() && (
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
          onInput={handleInput}
          onSubmit={handleSubmit as any}
        />
      </box>
    </box>
  );
}
