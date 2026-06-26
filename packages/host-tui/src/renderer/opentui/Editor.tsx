// ============================================================================
// Editor — multiline textarea input with shift+enter newlines and emoji-free image paste.
// Autocomplete state managed locally by EditorAutocompleteController.
// ============================================================================

import type { KeyEvent, TextareaRenderable } from "@opentui/core";
import { debugTrace, type ImageContent, joinPath } from "piko-host-runtime";
import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import type { AutocompleteItem } from "../../autocomplete/types.js";
import { EditorAutocompleteController } from "../../editor/editor-autocomplete-controller.js";
import type { EditorAutocompleteState } from "../../editor/editor-autocomplete-state.js";
import { createEmptyAutocompleteState } from "../../editor/editor-autocomplete-state.js";
import type { KeyEvent as FocusKeyEvent } from "../../focus/types.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import type { ActionService } from "./action-service.js";
import { CommandAutocomplete } from "./autocomplete/CommandAutocomplete.js";
import { useTheme } from "./theme-context.js";

export interface EditorProps {
  draft: string;
  draftRevision: number;
  onDraftChange: (text: string) => void;
  actionSvc: ActionService;
  controller: TuiController;
  disabled: boolean;
  unfocused?: boolean;
  placeholder?: string;
}

const AUTOCOMPLETE_MAX_VISIBLE = 8;
const AUTOCOMPLETE_HEIGHT = AUTOCOMPLETE_MAX_VISIBLE + 1;

function tmpDir(): string {
  return Bun.env.TMPDIR ?? Bun.env.TEMP ?? Bun.env.TMP ?? "/tmp";
}

function escapeAppleScriptString(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

async function extractClipboardImage(): Promise<string | null> {
  const tempPath = joinPath(
    tmpDir(),
    `piko-clip-${Date.now()}-${Math.random().toString(36).substring(2, 9)}.png`,
  );
  try {
    let pngBytes: Uint8Array | undefined;
    if (process.platform === "darwin") {
      const proc = Bun.spawnSync(
        [
          "osascript",
          "-e",
          `write (the clipboard as «class PNGf») to (open for access POSIX file "${escapeAppleScriptString(
            tempPath,
          )}" with write permission)`,
        ],
        {
          stdin: "ignore",
          stdout: "ignore",
          stderr: "ignore",
        },
      );
      if (proc.exitCode === 0 && (await Bun.file(tempPath).exists())) {
        const file = Bun.file(tempPath);
        if (file.size > 0) return tempPath;
      }
    } else if (process.platform === "linux") {
      const wlPaste = Bun.spawnSync(["wl-paste", "--type", "image/png"], {
        stdin: "ignore",
        stdout: "pipe",
        stderr: "ignore",
      });
      if (wlPaste.exitCode === 0 && wlPaste.stdout.length > 0) {
        pngBytes = wlPaste.stdout;
      } else {
        const xclip = Bun.spawnSync(["xclip", "-selection", "clipboard", "-t", "image/png", "-o"], {
          stdin: "ignore",
          stdout: "pipe",
          stderr: "ignore",
        });
        if (xclip.exitCode === 0 && xclip.stdout.length > 0) {
          pngBytes = xclip.stdout;
        }
      }
    }

    if (pngBytes && pngBytes.length > 0) {
      await Bun.write(tempPath, pngBytes);
      return tempPath;
    }
  } catch {
    // Clipboard extraction failed
  }
  return null;
}

export function Editor(props: EditorProps) {
  const theme = useTheme();
  const { actionSvc, controller } = props;
  let textareaRef: TextareaRenderable | undefined;

  let lastAppliedRevision = -1;

  const applyLatestDraft = () => {
    if (!textareaRef) return;
    textareaRef.setText(props.draft);
    textareaRef.cursorOffset = props.draft.length;
    textareaRef.requestRender();
    lastAppliedRevision = props.draftRevision;
  };

  createEffect(() => {
    const revision = props.draftRevision;
    if (!textareaRef || revision === lastAppliedRevision) return;
    textareaRef.setText(props.draft);
    textareaRef.cursorOffset = props.draft.length;
    textareaRef.requestRender();
    lastAppliedRevision = revision;
  });

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
  controller.setEditorTextAccessor(() => textareaRef?.plainText ?? "");
  onCleanup(() => {
    ac().dispose();
    controller.setAutocompleteController(null);
    controller.setAutocompleteKeyHandler(null);
    controller.setEditorTextAccessor(null);
  });

  const showSlashMenu = () => {
    const text = props.draft.trimStart();
    return !props.disabled && (text.startsWith("/") || text.includes("@"));
  };

  const syncSlashItems = (): AutocompleteItem[] => {
    const text = props.draft;
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
    const text = props.draft;
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
        props.onDraftChange(result.input);
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
      const slashDraft = props.draft.trimStart();
      if (items.length > 0 && slashDraft.startsWith("/") && !/\s/.test(slashDraft)) {
        const selected =
          ac().getSelectedItem() ?? items[Math.min(acState().selectedIndex, items.length - 1)];
        if (selected) {
          const cmd = selected.value;
          ac().cancel();
          textareaRef?.clear();
          props.onDraftChange("");
          setAttachments(new Map());
          setAttachmentCounter(0);
          pastes.clear();
          controller.executeSlash(cmd);
          return true;
        }
      }
    }
    return false;
  }

  // ---- Clipboard Image Paste Handler ----
  const handleClipboardImagePaste = async () => {
    const filePath = await extractClipboardImage();
    if (!filePath) {
      controller.notifications.notify({
        message: "No image found in clipboard",
        severity: "warning",
      });
      return;
    }

    try {
      const data = await Bun.file(filePath).bytes();
      const base64Data = Buffer.from(data).toString("base64");
      const _size = data.length;

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
    // A focused OpenTUI textarea can consume Escape before the app-level
    // keyboard hook sees it. Handle interruption at the focused control as a
    // hard guarantee, while preserving Escape-to-close for autocomplete.
    if (event.name === "escape") {
      debugTrace({
        stage: "tui.escape.received",
        status: actionSvc.getState().stream.status,
      });
      if (autocompleteVisible()) {
        event.preventDefault();
        event.stopPropagation();
        ac().cancel();
        return;
      }
      if (actionSvc.getState().stream.status === "running") {
        event.preventDefault();
        event.stopPropagation();
        controller.handleInterrupt();
        return;
      }
    }

    // Alt+Enter → followUp: queue as follow-up message
    if (
      (event.name === "return" || event.name === "enter") &&
      event.option &&
      !event.ctrl &&
      !event.meta
    ) {
      event.preventDefault();
      const text = props.draft.trim();
      if (text) {
        actionSvc.followUp(text);
        textareaRef?.clear();
        props.onDraftChange("");
        setAttachments(new Map());
        setAttachmentCounter(0);
        pastes.clear();
      }
      return;
    }

    // Alt+Up → dequeue: restore all queued messages to editor
    if (event.name === "up" && event.option && !event.ctrl && !event.meta) {
      event.preventDefault();
      const queued = actionSvc.dequeue();
      if (queued) {
        if (textareaRef) {
          const current = textareaRef.plainText;
          const newVal = current ? `${queued}\n\n${current}` : queued;
          textareaRef.setText(newVal);
          props.onDraftChange(newVal);
          textareaRef.requestRender();
        }
      } else {
        controller.notifications.notify({
          message: "No queued messages to restore",
          severity: "info",
        });
      }
      return;
    }

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
          const text = props.draft;
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
    if (props.disabled) return;
    const rawText = props.draft;
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
          props.onDraftChange("");
          setAttachments(new Map());
          setAttachmentCounter(0);
          pastes.clear();
          return;
        }

        ac().cancel();
        textareaRef?.clear();
        props.onDraftChange("");
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
      props.onDraftChange("");
      setAttachments(new Map());
      setAttachmentCounter(0);
      pastes.clear();
      return;
    }

    // Normal submit
    ac().cancel();
    textareaRef?.clear();
    props.onDraftChange("");
    setAttachments(new Map());
    setAttachmentCounter(0);
    pastes.clear();
    actionSvc.submitPrompt(text, imagesList.length > 0 ? imagesList : undefined);
  }

  function handleInput(value: any): void {
    const textValue = typeof value === "string" ? value : (textareaRef?.plainText ?? "");
    props.onDraftChange(textValue);

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
      <Show when={autocompleteVisible()}>
        <box height={AUTOCOMPLETE_HEIGHT} flexShrink={0} overflow="hidden">
          <CommandAutocomplete
            items={visibleItems()}
            query={props.draft}
            selectedIndex={selectedIndex()}
            maxVisible={AUTOCOMPLETE_MAX_VISIBLE}
            onSelect={(item) => {
              const result = controller.autocomplete.applyCompletion(
                props.draft,
                props.draft.length,
                item,
                acState().prefix || props.draft.trimStart(),
              );
              props.onDraftChange(result.input);
              if (textareaRef) {
                textareaRef.setText(result.input);
                textareaRef.cursorOffset = result.cursor;
                handleInput(result.input);
                textareaRef.requestRender();
              }
            }}
            onCancel={() => ac().cancel()}
          />
        </box>
      </Show>

      {/* Input */}
      <box border={["top", "bottom"]} borderColor={theme.color("border.muted")}>
        <textarea
          ref={(el: TextareaRenderable) => {
            textareaRef = el;
            applyLatestDraft();
          }}
          focused={!props.disabled && !(props.unfocused ?? false)}
          placeholder={
            props.placeholder ??
            (props.disabled ? "Running..." : "Ask a question, or type '/' for commands...")
          }
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
