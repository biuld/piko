# Keybindings System

## Overview

The Keybindings System is the central hub for keyboard shortcut definition, custom configuration, and event routing within the piko terminal user interface. It ensures that standard keypresses map consistently to actions (like navigating, submitting prompts, opening overlay panels, and approving actions) while allowing full customization via user-defined configuration files.

## Input Routing & Context Scoping

Keyboard events flow through a three-priority sequence to handle both global shortcuts and context-sensitive local keys:

1. **Priority 1: Global Shortcuts (Esc/Enter)**
   - Certain universal keys are intercepted first. E.g., `Esc` closes active overlays or cancels the running turn, while `Enter` accepts autocomplete suggestions if they are open.
2. **Priority 2: Focus Owner (Mode-Specific Routing)**
   - The active panel/mode (such as the Session Tree, Model Selector, or Approval Panel) gets the opportunity to process the key first.
   - **Context-Scoped Routing**: Keys mapped for a specific overlay (e.g., `ctrl+a` or `ctrl+d` for Approvals) are *only* active when that specific overlay has focus. When the overlay is closed, these keys fall back to their normal global or editor behaviors (e.g., `ctrl+a` goes to line start in the editor).
3. **Priority 3: Editor Fallback**
   - If no overlay panel consumes the key, the key event is routed to the Editor for text composition, history browsing, or cursor movement.

## Default Keybindings Map

Below is the organized list of all default keyboard shortcuts in piko, aligned with `pi-mono`.

### 1. Editor Navigation & Editing
These keys are active when the Editor is focused and not blocked by active overlays:

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `tui.editor.cursorUp` | `up` | Move cursor up |
| `tui.editor.cursorDown` | `down` | Move cursor down |
| `tui.editor.cursorLeft` | `left`, `ctrl+b` | Move cursor left one character |
| `tui.editor.cursorRight` | `right`, `ctrl+f` | Move cursor right one character |
| `tui.editor.cursorWordLeft` | `alt+left`, `ctrl+left`, `alt+b` | Move cursor left one word |
| `tui.editor.cursorWordRight` | `alt+right`, `ctrl+right`, `alt+f` | Move cursor right one word |
| `tui.editor.cursorLineStart` | `home`, `ctrl+a` | Move cursor to the start of the line |
| `tui.editor.cursorLineEnd` | `end`, `ctrl+e` | Move cursor to the end of the line |
| `tui.editor.jumpForward` | `ctrl+]` | Jump forward to a specific character |
| `tui.editor.jumpBackward` | `ctrl+alt+]` | Jump backward to a specific character |
| `tui.editor.pageUp` | `pageup` | Scroll up one page |
| `tui.editor.pageDown` | `pagedown` | Scroll down one page |
| `tui.editor.deleteCharBackward` | `backspace` | Delete the character backward |
| `tui.editor.deleteCharForward` | `delete`, `ctrl+d` | Delete the character forward |
| `tui.editor.deleteWordBackward` | `ctrl+w`, `alt+backspace` | Delete the word backward |
| `tui.editor.deleteWordForward` | `alt+d`, `alt+delete` | Delete the word forward |
| `tui.editor.deleteToLineStart` | `ctrl+u` | Delete text from cursor to start of line |
| `tui.editor.deleteToLineEnd` | `ctrl+k` | Delete text from cursor to end of line |
| `tui.editor.yank` | `ctrl+y` | Paste deleted text from the kill ring (Yank) |
| `tui.editor.yankPop` | `alt+y` | Cycle through previously deleted text |
| `tui.editor.undo` | `ctrl+-` | Undo the last editor action |
| `tui.input.newLine` | `shift+enter`, `ctrl+j` | Insert a newline into the prompt |
| `tui.input.submit` | `enter` | Submit the prompt to the LLM |
| `tui.input.tab` | `tab` | Complete the selected suggestion |
| `tui.input.copy` | `ctrl+c` | Copy the selected text to the system clipboard |

### 2. Selection & Navigation (List Overlays)
These keys are active within filterable lists (e.g., Command Palette, Model Selector, Session List):

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `tui.select.up` | `up` | Move selection highlight up |
| `tui.select.down` | `down` | Move selection highlight down |
| `tui.select.pageUp` | `pageup` | Move selection highlight up one page |
| `tui.select.pageDown` | `pagedown` | Move selection highlight down one page |
| `tui.select.confirm` | `enter` | Confirm the highlighted option |
| `tui.select.cancel` | `escape`, `ctrl+c` | Close the selection list without choosing |

### 3. Application Actions & Overlays
Global or editor-level shortcuts that open overlays, manage sessions, or interrupt/exit the program:

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `app.interrupt` | `escape` | Cancel/abort the current model streaming or task |
| `app.clear` | `ctrl+c` | Clear the editor contents |
| `app.exit` | `ctrl+q` | Exit the piko TUI application |
| `app.suspend` | `ctrl+z` | Suspend the app to background (Unix/macOS only) |
| `app.thinking.cycle` | `shift+tab` | Cycle through reasoning/thinking levels |
| `app.model.cycleForward` | `ctrl+p` | Cycle forward to the next model in active list |
| `app.model.cycleBackward` | `shift+ctrl+p` | Cycle backward to the previous model in active list |
| `app.model.select` | `ctrl+l` (or `f3`) | Open the model selector overlay |
| `app.tools.expand` | `ctrl+o` | Toggle tools details expansion |
| `app.thinking.toggle` | `ctrl+t` | Toggle visibility of thinking/reasoning blocks |
| `app.session.toggleNamedFilter` | `ctrl+n` | Cycle through named session filters in Session list |
| `app.editor.external` | `ctrl+g` | Open active editor text in an external editor |
| `app.message.followUp` | `alt+enter` | Queue follow-up message without executing |
| `app.message.dequeue` | `alt+up` | Restore a previously queued follow-up message |
| `app.clipboard.pasteImage` | `ctrl+v` (`alt+v` on Windows) | Paste an image from the clipboard |
| `app.session.new` | *None* | Start a new chat session |
| `app.session.tree` | `f2` (or *None*) | Open the session list/tree view |
| `app.session.fork` | *None* | Fork the current session from the selected point |
| `app.session.resume` | *None* | Resume a selected session |

### 4. Tree-Panel Navigation
These keys are active when the Session Tree panel is focused:

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `app.tree.foldOrUp` | `ctrl+left`, `alt+left` | Fold tree branch or move to parent node |
| `app.tree.unfoldOrDown` | `ctrl+right`, `alt+right` | Unfold tree branch or move down to child |
| `app.tree.editLabel` | `shift+l` | Edit the label of the highlighted session node |
| `app.tree.toggleLabelTimestamp` | `shift+t` | Toggle displaying timestamps next to tree node labels |
| `app.session.togglePath` | `ctrl+p` | Toggle displaying directory path next to session labels |
| `app.session.toggleSort` | `ctrl+s` | Toggle sorting sessions by timestamp vs name |
| `app.session.rename` | `ctrl+r` | Rename the highlighted session |
| `app.session.delete` | `ctrl+d` | Delete the highlighted session |
| `app.session.deleteNoninvasive` | `ctrl+backspace` | Delete the highlighted session (only if query is empty) |
| `app.tree.filter.default` | `ctrl+d` | Reset session tree filter to default view |
| `app.tree.filter.noTools` | `ctrl+t` | Filter tree to hide tool result nodes |
| `app.tree.filter.userOnly` | `ctrl+u` | Filter tree to show user-authored messages only |
| `app.tree.filter.labeledOnly` | `ctrl+l` | Filter tree to show only labeled session nodes |
| `app.tree.filter.all` | `ctrl+a` | Disable all filters and show all tree nodes |
| `app.tree.filter.cycleForward` | `ctrl+o` | Cycle forward through tree filter modes |
| `app.tree.filter.cycleBackward` | `shift+ctrl+o` | Cycle backward through tree filter modes |

### 5. Model Selector Panel Actions
These keys are active when the Model Selector overlay panel is open:

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `app.models.save` | `ctrl+s` | Save the current model configuration |
| `app.models.enableAll` | `ctrl+a` | Enable all available models |
| `app.models.clearAll` | `ctrl+x` | Disable/deselect all models |
| `app.models.toggleProvider` | `ctrl+p` | Toggle all models for the selected provider |
| `app.models.reorderUp` | `alt+up` | Move highlighted model up in priority order |
| `app.models.reorderDown` | `alt+down` | Move highlighted model down in priority order |

### 6. Approval Mode Panel Actions
These keys are active when the Approval Panel is shown:

| Keybinding ID | Default Key(s) | Description |
|---|---|---|
| `app.approval.accept` | `enter` | Accept the pending tool/command execution |
| `app.approval.acceptSession` | `a` | Accept execution for the remainder of this session |
| `app.approval.acceptWorkspace` | `w` | Accept execution for all sessions in this workspace |
| `app.approval.acceptPermanent` | `p` | Permanently trust/accept this tool/command |
| `app.approval.decline` | `escape`, `ctrl+d` | Decline/reject the pending execution |

---

## Configuration

Custom keybindings can be defined globally and per-project using a JSON file:

- **Global Config**: `~/.piko/keybindings.json`
- **Project Config**: `<working-dir>/.piko/keybindings.json`

### JSON Structure

Users define customization by specifying bindings mapped to `KeyId` strings (e.g., `"ctrl+c"`, `"escape"`, `"shift+enter"`):

```json
{
  "bindings": {
    "app.exit": "ctrl+q",
    "app.clear": "ctrl+c",
    "tui.input.newLine": ["shift+enter", "ctrl+j"],
    "tui.editor.cursorLineStart": ["home", "ctrl+a"]
  }
}
```

## Non-goals

- An interactive, in-app keyboard shortcut re-binder GUI.
- Defining multi-key chord sequences (e.g., `ctrl+k ctrl+c`).
- Overriding system-level mouse scrolling or selection behaviors.
