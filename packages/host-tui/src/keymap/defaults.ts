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
    scope: "global",
  },
  {
    id: "app.clear",
    keys: { key: "c", ctrl: true },
    scope: "global",
  },
  {
    id: "app.exit",
    keys: { key: "d", ctrl: true },
    requiresIdle: true,
    scope: "global",
  },
  {
    id: "app.model.select",
    keys: { key: "l", ctrl: true },
    scope: "global",
  },
  {
    id: "app.model.cycleForward",
    keys: { key: "p", ctrl: true },
    scope: "global",
  },
  {
    id: "app.model.cycleBackward",
    keys: { key: "n", ctrl: true },
    scope: "global",
  },
  {
    id: "app.tools.expand",
    keys: { key: "o", ctrl: true },
    scope: "global",
  },
  {
    id: "app.agent.toggleExpand",
    keys: { key: "e", ctrl: true },
    scope: "global",
  },
  {
    id: "app.thinking.toggle",
    keys: { key: "r", ctrl: true },
    scope: "global",
  },
  {
    id: "app.session.tree",
    keys: { key: "t", ctrl: true },
    requiresIdle: true,
    scope: "global",
  },
  // App bindings without default keys (registered for labels + overrides)
  { id: "app.suspend", keys: { key: "" }, scope: "global" },
  { id: "app.thinking.cycle", keys: { key: "" }, scope: "global" },
  { id: "app.editor.external", keys: { key: "" }, scope: "global" },
  { id: "app.message.followUp", keys: { key: "" }, scope: "global" },
  { id: "app.message.dequeue", keys: { key: "" }, scope: "global" },
  { id: "app.clipboard.pasteImage", keys: { key: "" }, scope: "global" },
  { id: "app.session.new", keys: { key: "" }, scope: "global" },
  { id: "app.session.fork", keys: { key: "" }, scope: "global" },
  { id: "app.session.resume", keys: { key: "" }, scope: "global" },
  { id: "app.session.togglePath", keys: { key: "" }, scope: "global" },
  { id: "app.session.toggleSort", keys: { key: "" }, scope: "global" },
  { id: "app.session.rename", keys: { key: "" }, scope: "global" },
  { id: "app.session.delete", keys: { key: "" }, scope: "global" },
  { id: "app.models.save", keys: { key: "" }, scope: "global" },
  { id: "app.models.enableAll", keys: { key: "" }, scope: "global" },
  { id: "app.models.clearAll", keys: { key: "" }, scope: "global" },
  { id: "app.models.toggleProvider", keys: { key: "" }, scope: "global" },
  { id: "app.models.reorderUp", keys: { key: "" }, scope: "global" },
  { id: "app.models.reorderDown", keys: { key: "" }, scope: "global" },

  // ---- TUI editor bindings ----
  {
    id: "tui.editor.cursorUp",
    keys: { key: "up" },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorDown",
    keys: { key: "down" },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorLeft",
    keys: { key: "left" },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorRight",
    keys: { key: "right" },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorWordLeft",
    keys: { key: "left", alt: true },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorWordRight",
    keys: { key: "right", alt: true },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorLineStart",
    keys: { key: "home" },
    scope: "editor",
  },
  {
    id: "tui.editor.cursorLineEnd",
    keys: { key: "end" },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteCharBackward",
    keys: { key: "backspace" },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteCharForward",
    keys: { key: "delete" },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteWordBackward",
    keys: { key: "backspace", alt: true },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteWordForward",
    keys: { key: "delete", alt: true },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteToLineStart",
    keys: { key: "u", ctrl: true },
    scope: "editor",
  },
  {
    id: "tui.editor.deleteToLineEnd",
    keys: { key: "k", ctrl: true },
    scope: "editor",
  },
  {
    id: "tui.editor.yank",
    keys: { key: "y", ctrl: true },
    scope: "editor",
  },
  {
    id: "tui.editor.undo",
    keys: { key: "_", ctrl: true },
    scope: "editor",
  },

  // ---- TUI input bindings ----
  {
    id: "tui.input.submit",
    keys: { key: "enter" },
    scope: "editor",
  },
  {
    id: "tui.input.newLine",
    keys: { key: "enter", shift: true },
    scope: "editor",
  },
  {
    id: "tui.input.tab",
    keys: { key: "tab" },
    scope: "editor",
  },

  // ---- TUI selector bindings ----
  {
    id: "tui.select.up",
    keys: { key: "up" },
    scope: "selector",
  },
  {
    id: "tui.select.down",
    keys: { key: "down" },
    scope: "selector",
  },
  {
    id: "tui.select.pageUp",
    keys: { key: "pageup" },
    scope: "selector",
  },
  {
    id: "tui.select.pageDown",
    keys: { key: "pagedown" },
    scope: "selector",
  },
  {
    id: "tui.select.confirm",
    keys: { key: "enter" },
    scope: "selector",
  },
  {
    id: "tui.select.cancel",
    keys: { key: "escape" },
    scope: "selector",
  },

  // ---- TUI timeline bindings ----
  {
    id: "tui.timeline.up",
    keys: { key: "up" },
    scope: "timeline",
  },
  {
    id: "tui.timeline.down",
    keys: { key: "down" },
    scope: "timeline",
  },
  {
    id: "tui.timeline.jumpLatest",
    keys: { key: "end" },
    scope: "timeline",
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
  "app.agent.toggleExpand": "^E Agent",
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
