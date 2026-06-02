// ============================================================================
// Keymap defaults — pi-compatible default keybindings
// ============================================================================

import type { KeybindingEntry, KeybindingId } from "./types.js";

/**
 * Built-in default keybindings matching pi's behavior.
 */
export const DEFAULT_KEYBINDINGS: KeybindingEntry[] = [
  // ---- App bindings ----
  {
    id: "app.interrupt",
    keys: { key: "escape" },
  },
  {
    id: "app.exit",
    keys: { key: "d", ctrl: true },
    requiresIdle: true,
  },
  {
    id: "app.model.select",
    keys: { key: "l", ctrl: true },
  },
  {
    id: "app.model.cycleForward",
    keys: { key: "p", ctrl: true },
  },
  {
    id: "app.model.cycleBackward",
    keys: { key: "n", ctrl: true },
  },
  {
    id: "app.tools.expand",
    keys: { key: "o", ctrl: true },
  },
  {
    id: "app.thinking.toggle",
    keys: { key: "r", ctrl: true },
  },
  {
    id: "app.session.tree",
    keys: { key: "t", ctrl: true },
    requiresIdle: true,
  },

  // ---- TUI input bindings ----
  {
    id: "tui.input.submit",
    keys: { key: "enter" },
  },
  {
    id: "tui.input.newLine",
    keys: { key: "enter", shift: true },
  },
  {
    id: "tui.input.tab",
    keys: { key: "tab" },
  },

  // ---- TUI selector bindings ----
  {
    id: "tui.select.up",
    keys: { key: "up" },
  },
  {
    id: "tui.select.down",
    keys: { key: "down" },
  },
  {
    id: "tui.select.pageUp",
    keys: { key: "pageup" },
  },
  {
    id: "tui.select.pageDown",
    keys: { key: "pagedown" },
  },
  {
    id: "tui.select.confirm",
    keys: { key: "enter" },
  },
  {
    id: "tui.select.cancel",
    keys: { key: "escape" },
  },

  // ---- TUI autocomplete bindings ----
  {
    id: "tui.autocomplete.accept",
    keys: { key: "tab" },
  },
  {
    id: "tui.autocomplete.cancel",
    keys: { key: "escape" },
  },
  {
    id: "tui.autocomplete.navigateUp",
    keys: { key: "up" },
  },
  {
    id: "tui.autocomplete.navigateDown",
    keys: { key: "down" },
  },

  // ---- TUI timeline bindings ----
  {
    id: "tui.timeline.up",
    keys: { key: "up" },
  },
  {
    id: "tui.timeline.down",
    keys: { key: "down" },
  },
  {
    id: "tui.timeline.jumpLatest",
    keys: { key: "end" },
  },
];

/**
 * Human-readable display labels for keybinding IDs.
 */
export const KEYBINDING_LABELS: Record<KeybindingId, string> = {
  // TUI editor
  "tui.editor.cursorUp": "↑",
  "tui.editor.cursorDown": "↓",
  "tui.editor.cursorLeft": "←",
  "tui.editor.cursorRight": "→",
  "tui.editor.cursorWordLeft": "⌥←",
  "tui.editor.cursorWordRight": "⌥→",
  "tui.editor.cursorLineStart": "Home",
  "tui.editor.cursorLineEnd": "End",
  "tui.editor.pageUp": "PgUp",
  "tui.editor.pageDown": "PgDn",
  "tui.editor.deleteCharBackward": "⌫",
  "tui.editor.deleteCharForward": "Del",
  "tui.editor.deleteWordBackward": "⌥⌫",
  "tui.editor.deleteWordForward": "⌥Del",
  "tui.editor.deleteToLineStart": "^U",
  "tui.editor.deleteToLineEnd": "^K",
  "tui.editor.yank": "^Y",
  "tui.editor.undo": "^_",
  // TUI input
  "tui.input.newLine": "⇧↵",
  "tui.input.submit": "↵",
  "tui.input.tab": "Tab",
  "tui.input.copy": "^C",
  // TUI select
  "tui.select.up": "↑",
  "tui.select.down": "↓",
  "tui.select.pageUp": "PgUp",
  "tui.select.pageDown": "PgDn",
  "tui.select.confirm": "↵ Select",
  "tui.select.cancel": "Esc Cancel",
  // TUI timeline
  "tui.timeline.up": "↑",
  "tui.timeline.down": "↓",
  "tui.timeline.pageUp": "PgUp",
  "tui.timeline.pageDown": "PgDn",
  "tui.timeline.jumpLatest": "End",
  "tui.timeline.expandTool": "↵ Expand",
  "tui.timeline.collapseTool": "↵ Collapse",
  // TUI autocomplete
  "tui.autocomplete.accept": "Tab Accept",
  "tui.autocomplete.cancel": "Esc Cancel",
  "tui.autocomplete.navigateUp": "↑",
  "tui.autocomplete.navigateDown": "↓",
  // App
  "app.interrupt": "Esc Interrupt",
  "app.clear": "^C Clear",
  "app.exit": "^D Exit",
  "app.suspend": "^Z",
  "app.thinking.cycle": "^R",
  "app.model.cycleForward": "^P",
  "app.model.cycleBackward": "^N",
  "app.model.select": "^L Model",
  "app.tools.expand": "^O Expand",
  "app.thinking.toggle": "^R Think",
  "app.editor.external": "^E Edit",
  "app.message.followUp": "^F FollowUp",
  "app.message.dequeue": "^G Dequeue",
  "app.clipboard.pasteImage": "^V Paste",
  "app.session.new": "",
  "app.session.tree": "^T Sessions",
  "app.session.fork": "",
  "app.session.resume": "^R Resume",
  "app.session.togglePath": "",
  "app.session.toggleSort": "",
  "app.session.rename": "",
  "app.session.delete": "",
  "app.models.save": "",
  "app.models.enableAll": "",
  "app.models.clearAll": "",
  "app.models.toggleProvider": "",
  "app.models.reorderUp": "",
  "app.models.reorderDown": "",
};

/**
 * Format a key combo for human display.
 */
export function formatKeyCombo(combo: {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  meta?: boolean;
}): string {
  const parts: string[] = [];
  if (combo.ctrl) parts.push("^");
  if (combo.alt) parts.push("⌥");
  if (combo.meta) parts.push("⌘");
  if (combo.shift) parts.push("⇧");
  const key = combo.key
    .replace("enter", "↵")
    .replace("escape", "Esc")
    .replace("backspace", "⌫")
    .replace("delete", "Del")
    .replace("tab", "Tab")
    .replace("space", "Space")
    .replace("up", "↑")
    .replace("down", "↓")
    .replace("left", "←")
    .replace("right", "→")
    .replace("pageup", "PgUp")
    .replace("pagedown", "PgDn")
    .replace("home", "Home")
    .replace("end", "End");
  parts.push(key);
  return parts.join("");
}
