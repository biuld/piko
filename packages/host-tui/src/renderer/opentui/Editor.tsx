// ============================================================================
// Editor — multiline textarea input with shift+enter newlines and emoji-free image paste.
// Autocomplete state managed locally by EditorAutocompleteController.
// ============================================================================

import type { TextareaRenderable, KeyEvent } from "@opentui/core";
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
import type { ImageContent } from "piko-engine-protocol";
import { execSync } from "child_process";
import * as path from "path";
import * as os from "os";
import * as fs from "fs";

export interface EditorProps {
  actionSvc: ActionService;
  controller: TuiController;
  disabled: boolean;
  unfocused?: boolean;
}

const AUTOCOMPLETE_MAX_VISIBLE = 8;
const AUTOCOMPLETE_HEIGHT = AUTOCOMPLETE_MAX_VISIBLE + 1;

function extractClipboardImage(): string | null {
  const tempPath = path.join(
    os.tmpdir(),
    `piko-clip-${Date.now()}-${Math.random().toString(36).substring(2, 9)}.png`,
  );
  try {
    if (process.platform === "darwin") {
      execSync(
        `osascript -e 'write (the clipboard as «class PNGf») to (open for access POSIX file "${tempPath}" with write permission)'`,
        { stdio: "ignore" },
      );
    } else if (process.platform === "linux") {
      try {
        execSync(`wl-paste --type image/png > "${tempPath}"`, { stdio: "ignore" });
      } catch {
        execSync(`xclip -selection clipboard -t image/png -o > "${tempPath}"`, { stdio: "ignore" });
      }
    }
    if (fs.existsSync(tempPath) && fs.statSync(tempPath).size > 0) {
      return tempPath;
    }
  } catch {
    // Clipboard extraction failed
  }
  return null;
}

export function Editor(props: EditorProps) {
  const theme = useTheme();
  const { actionSvc, controller, disabled, unfocused = false } = props;
  let textareaRef: TextareaRenderable | undefined;
  const [draft, setDraft] = createSignal("");

  // ---- Attachments Map ----
  const [attachments, setAttachments] = createSignal<
    Map<string, { filePath: string; data: string; mimeType: string }>
  >(new Map());
  const [attachmentCounter, setAttachmentCounter] = createSignal(0);

  // ---- Pastes Map ----
  const pastes = new Map<number, string>();
  let pasteCounter = 0;

  // ---- Local autocomplete controller ----
  const [acState, setAcState] = createSignal<EditorAutocompleteState>(
    createEmptyAutocompleteState(),
  );
  const [editorAc] = createSignal(
    new EditorAutocompleteController(
      controller.autocomplete,
      (s) => setAcState(s),
      undefined,
      (input: string): AutocompleteItem[] => {
        if (input.trimStart().startsWith("/")) {
          return controller.getAutocomplete(input);
        }
        return [];
      },
    ),
  );
  const ac = () => editorAc();

  controller.setAutocompleteController(ac());
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

  createEffect(() => {
    const text = draft();
    if (showSlashMenu()) {
      ac().query(text, text.length);
    } else {
      ac().cancel();
    }
  });

  // ---- Local autocomplete key handler ----
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
        if (textareaRef) {
          textareaRef.setText(result.input);
          textareaRef.cursorOffset = result.cursor;
          handleInput(result.input);
          textareaRef.requestRender();
        }
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
          textareaRef?.clear();
          setDraft("");
          controller.executeSlash(cmd);
          return true;
        }
      }
    }
    return false;
  }

  // ---- Clipboard Image Paste Handler ----
  const handleClipboardImagePaste = () => {
    const filePath = extractClipboardImage();
    if (!filePath) {
      controller.notifications.notify({
        message: "No image found in clipboard",
        severity: "warning",
      });
      return;
    }

    try {
      const data = fs.readFileSync(filePath);
      const base64Data = data.toString("base64");
      const size = data.length;

      const newId = (attachmentCounter() + 1).toString();
      setAttachmentCounter(newId as any as number);

      const nextMap = new Map(attachments());
      nextMap.set(newId, {
        filePath,
        data: base64Data,
        mimeType: "image/png",
      });
      setAttachments(nextMap);

      const placeholder = `[image #${newId}]`;
      if (textareaRef) {
        textareaRef.insertText(placeholder);
        handleInput(textareaRef.plainText);
        textareaRef.requestRender();
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      controller.notifications.notify({
        message: `Failed to read clipboard image: ${msg}`,
        severity: "error",
      });
    }
  };

  // ---- Paste Event (Large Text Placeholders) ----
  const handlePasteEvent = (event: any) => {
    try {
      const text = new TextDecoder().decode(event.bytes);
      const lines = text.split("\n");
      if (lines.length > 10 || text.length > 1000) {
        event.preventDefault();

        pasteCounter++;
        const id = pasteCounter;
        pastes.set(id, text);

        const placeholder = `[paste #${id}]`;
        if (textareaRef) {
          textareaRef.insertText(placeholder);
          handleInput(textareaRef.plainText);
          textareaRef.requestRender();
        }
      }
    } catch {
      // Let default paste happen
    }
  };

  // ---- Keydown Interceptor ----
  const handleKeyDown = (event: KeyEvent) => {
    // Check for paste image shortcut: Ctrl+V
    if (event.ctrl && event.name === "v") {
      event.preventDefault();
      handleClipboardImagePaste();
      return;
    }

    // Backslash-enter newline fallback for terminals without Shift+Enter
    if (event.name === "return" || event.name === "enter") {
      if (!event.shift && !event.ctrl && !event.meta) {
        if (textareaRef) {
          const offset = textareaRef.cursorOffset;
          const text = draft();
          const charBefore = offset > 0 ? text[offset - 1] : "";
          if (charBefore === "\\") {
            event.preventDefault();
            textareaRef.deleteCharBackward();
            textareaRef.newLine();
          }
        }
      }
    }
  };

  // ---- Submit ----
  function handleSubmit(): void {
    if (disabled) return;
    const rawText = draft();
    if (!rawText.trim()) return;

    // Expand text placeholders
    let text = rawText;
    for (const [id, content] of pastes.entries()) {
      const regex = new RegExp(`\\[paste #${id}\\]`, "g");
      text = text.replace(regex, () => content);
    }

    // Process image attachments
    const imagesList: ImageContent[] = [];
    const activeAttachments = attachments();
    for (const [id, info] of activeAttachments.entries()) {
      const placeholder = `[image #${id}]`;
      if (text.includes(placeholder)) {
        imagesList.push({
          type: "image",
          mimeType: info.mimeType,
          data: info.data,
        });
        text = text.replace(placeholder, "");
      }
    }

    text = text.trim();
    if (!text && imagesList.length > 0) {
      text = "Analyze the attached image(s).";
    }

    if (!text && imagesList.length === 0) return;

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
          textareaRef?.clear();
          setDraft("");
          setAttachments(new Map());
          setAttachmentCounter(0);
          pastes.clear();
          return;
        }

        ac().cancel();
        textareaRef?.clear();
        setDraft("");
        setAttachments(new Map());
        setAttachmentCounter(0);
        pastes.clear();
        controller.executeSlash(text);
        return;
      }

      // Unknown slash command
      controller.notifications.notify({
        message: `Unknown command: ${text}`,
        severity: "error",
      });
      ac().cancel();
      textareaRef?.clear();
      setDraft("");
      setAttachments(new Map());
      setAttachmentCounter(0);
      pastes.clear();
      return;
    }

    // Normal submit
    ac().cancel();
    textareaRef?.clear();
    setDraft("");
    setAttachments(new Map());
    setAttachmentCounter(0);
    pastes.clear();
    actionSvc.submitPrompt(text, imagesList.length > 0 ? imagesList : undefined);
  }

  function handleInput(value: any): void {
    const textValue = typeof value === "string" ? value : (textareaRef?.plainText ?? "");
    setDraft(textValue);

    // Sync attachments: remove any attachment whose tag was deleted from the text
    let changed = false;
    const nextMap = new Map(attachments());
    for (const [id] of nextMap.entries()) {
      if (!textValue.includes(`[image #${id}]`)) {
        nextMap.delete(id);
        changed = true;
      }
    }
    if (changed) {
      setAttachments(nextMap);
    }
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
              if (textareaRef) {
                textareaRef.setText(result.input);
                textareaRef.cursorOffset = result.cursor;
                handleInput(result.input);
                textareaRef.requestRender();
              }
            }}
            onCancel={() => ac().cancel()}
          />
        )}
      </box>

      {/* Input */}
      <box border={["top", "bottom"]} borderColor={theme.color("border.muted")}>
        <textarea
          ref={(el: TextareaRenderable) => {
            textareaRef = el;
          }}
          focused={!disabled && !unfocused}
          placeholder={disabled ? "Running..." : "Ask a question, or type '/' for commands..."}
          onContentChange={handleInput as any}
          onSubmit={handleSubmit}
          keyBindings={[
            { name: "return", action: "submit" },
            { name: "kpenter", action: "submit" },
            { name: "linefeed", action: "submit" },
            { name: "return", shift: true, action: "newline" },
            { name: "kpenter", shift: true, action: "newline" },
          ]}
          onKeyDown={handleKeyDown}
          onPaste={handlePasteEvent as any}
        />
      </box>
    </box>
  );
}
