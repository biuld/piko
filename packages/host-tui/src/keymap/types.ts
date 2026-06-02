// ============================================================================
// Keymap types — pi-compatible keybinding IDs and definitions
// ============================================================================

export type TuiKeybindingId =
  // Editor cursor movement
  | "tui.editor.cursorUp"
  | "tui.editor.cursorDown"
  | "tui.editor.cursorLeft"
  | "tui.editor.cursorRight"
  | "tui.editor.cursorWordLeft"
  | "tui.editor.cursorWordRight"
  | "tui.editor.cursorLineStart"
  | "tui.editor.cursorLineEnd"
  | "tui.editor.pageUp"
  | "tui.editor.pageDown"
  // Editor deletion
  | "tui.editor.deleteCharBackward"
  | "tui.editor.deleteCharForward"
  | "tui.editor.deleteWordBackward"
  | "tui.editor.deleteWordForward"
  | "tui.editor.deleteToLineStart"
  | "tui.editor.deleteToLineEnd"
  | "tui.editor.yank"
  | "tui.editor.undo"
  // Input actions
  | "tui.input.newLine"
  | "tui.input.submit"
  | "tui.input.tab"
  | "tui.input.copy"
  // Selector navigation
  | "tui.select.up"
  | "tui.select.down"
  | "tui.select.pageUp"
  | "tui.select.pageDown"
  | "tui.select.confirm"
  | "tui.select.cancel"
  // Timeline navigation
  | "tui.timeline.up"
  | "tui.timeline.down"
  | "tui.timeline.pageUp"
  | "tui.timeline.pageDown"
  | "tui.timeline.jumpLatest"
  | "tui.timeline.expandTool"
  | "tui.timeline.collapseTool"
  // Autocomplete
  | "tui.autocomplete.accept"
  | "tui.autocomplete.cancel"
  | "tui.autocomplete.navigateUp"
  | "tui.autocomplete.navigateDown";

export type AppKeybindingId =
  | "app.interrupt"
  | "app.clear"
  | "app.exit"
  | "app.suspend"
  | "app.thinking.cycle"
  | "app.model.cycleForward"
  | "app.model.cycleBackward"
  | "app.model.select"
  | "app.tools.expand"
  | "app.thinking.toggle"
  | "app.editor.external"
  | "app.message.followUp"
  | "app.message.dequeue"
  | "app.clipboard.pasteImage"
  | "app.session.new"
  | "app.session.tree"
  | "app.session.fork"
  | "app.session.resume"
  | "app.session.togglePath"
  | "app.session.toggleSort"
  | "app.session.rename"
  | "app.session.delete"
  | "app.models.save"
  | "app.models.enableAll"
  | "app.models.clearAll"
  | "app.models.toggleProvider"
  | "app.models.reorderUp"
  | "app.models.reorderDown";

export type KeybindingId = TuiKeybindingId | AppKeybindingId;

export interface KeyCombo {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  meta?: boolean;
}

export interface KeybindingEntry {
  id: KeybindingId;
  keys: KeyCombo;
  /** If true, this keybinding is inactive during a running stream */
  requiresIdle?: boolean;
}

export interface KeymapConfig {
  bindings: Record<string, string>;
}

export function keyComboMatches(
  combo: KeyCombo,
  keyName: string,
  ctrl: boolean,
  shift: boolean,
  alt: boolean,
  meta: boolean,
): boolean {
  if (combo.key !== keyName) return false;
  if ((combo.ctrl ?? false) !== ctrl) return false;
  if ((combo.shift ?? false) !== shift) return false;
  if ((combo.alt ?? false) !== alt) return false;
  if ((combo.meta ?? false) !== meta) return false;
  return true;
}
