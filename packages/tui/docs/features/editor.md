# Editor

## Overview

The Editor is the primary text input panel where users compose prompts, type
slash commands, and trigger autocompletions. It sits in the lower portion of the
Chat layout, between the AgentPanel/NotificationRow and the BottomBar.

The Editor is the **default focus owner**: when no overlay panel is active, all
keystrokes flow here. Partial overlays (model selector, command palette, etc.)
temporarily replace the Editor's slot — the Editor is not hidden behind a
panel, it is structurally absent during overlay sessions. When the overlay
closes, the Editor reappears with its content preserved.

## Layout

```
┌─────────────────────────────────────────────────────┐
│  (editor content, multi-line)                       │
└─────────────────────────────────────────────────────┘
```

- Fixed height (default 3 rows: 1 content line + top border + bottom border)
- Top and bottom borders only, no left/right border
- Border uses the muted chrome color from the active theme
- Terminal cursor tracks the text cursor position, clamped within the visible area

## Editing

### Text input

- **Typing**: printable characters insert at cursor position
- **Newline**: inserts a line break for multi-line input
- **Backspace**: deletes the character before the cursor
- **Delete**: deletes the character after the cursor

### Cursor movement

| Key | Action |
|-----|--------|
| Left / Right | Move cursor one character left/right within the current line |
| Home | Jump to beginning of current line |
| End | Jump to end of current line |

Cursor movement is bounded by line boundaries — left at column 0 stops, right
at end-of-line stops (does not wrap to adjacent lines).

### Multi-line editing

The Editor supports multiple lines. The visible area shows one line of content;
longer input scrolls within the fixed-height region. A newline key inserts a
line break.

## Submission

### Submitting a prompt

**Enter** submits the current content as a prompt to the LLM. The text is
trimmed of leading/trailing whitespace. If the trimmed text is empty, nothing
happens.

### Slash command interception

If the submitted text starts with `/`, the TUI attempts to parse it as a
**slash command** before sending it to the LLM. If the command is recognized, it
executes (e.g., opening a panel). If the command is *not* recognized, an error
notification is shown and the text is **not** forwarded to the LLM.

The set of available slash commands is defined by the slash command system,
not the Editor itself. See the slash commands documentation for the full list.

## History

The Editor remembers the last **100 submitted prompts**.

### Browsing history

| Key | Action |
|-----|--------|
| **Ctrl+P** | Previous entry (go back in history) |
| **Ctrl+E** | Next entry (go forward in history) |

- When not in history mode, pressing Ctrl+P loads the most recent submission.
- Continuing to press Ctrl+P goes further back (wraps around to the oldest
  entry when reaching the beginning).
- Ctrl+E moves forward. Past the newest entry, the editor returns to an empty
  draft.
- Any edit operation (typing, deleting, moving cursor) while browsing history
  immediately exits history mode — the current history text becomes a live draft
  that can be edited freely.

### Deduplication

Consecutive identical submissions are not stored twice. For example,
submitting "hello" twice only stores one entry.

## Autocompletion

The Editor provides two types of autocompletion, both triggered automatically
while typing.

### Slash command completion (`/`)

Typing `/` followed by one or more characters shows matching slash commands
in a suggestion list above the Editor. Available commands come from the
pluggable slash command system. Filtering matches the command name up to
the first space — arguments after the command name are ignored for matching.

### File path completion (`@`)

Typing `@` followed by a partial file or directory name shows matching paths
from the current working directory. Completion results are sorted
alphabetically. Directory matches get a trailing `/`.

### Completion UI

Suggestions appear in a dedicated area **above** the Editor (not floating on
top of it). The suggestion area shows:

- A title bar with the current position (`[M/N]`) and available controls
- One row per suggestion: `> command   description` (selected item marked with `>`)
- Selected item uses the theme's accent color, bold
- The area height grows with the number of completions (up to 8 rows max)

### Completion navigation and acceptance

| Key | Action |
|-----|--------|
| ↑ / ↓ | Move selection up/down in the completion list |
| Tab | Accept the selected completion (fills the text, keeps suggestions open) |
| Enter | Accept the selected completion and submit immediately |
| Esc | Cancel suggestions, return to normal editing |

You can continue typing while suggestions are visible — the list filters in
real time. When no items match, the suggestion area shows an empty state.

## Keyboard shortcuts from the Editor

The following global and editor-specific shortcuts are available while the
Editor has focus (no overlay active):

### Text editing

| Key | Action |
|-----|--------|
| Backspace | Delete character backward |
| Delete | Delete character forward |
| Ctrl+N | Insert newline |

### Submission and navigation

| Key | Action |
|-----|--------|
| Enter | Submit prompt |
| Ctrl+P | Previous history entry |
| Ctrl+E | Next history entry |
| F1 | Open help |
| F2 / Ctrl+R | Open session list |
| F3 | Open model selector |
| Ctrl+K | Open command palette |

### Quit

| Key | Action |
|-----|--------|
| Ctrl+C / Ctrl+Q | Quit the TUI |

## Esc key behavior from the Editor

The Esc key has a priority chain when the Editor has focus:

| Priority | Condition | Action |
|----------|-----------|--------|
| 1 | Overlay panel is active | Close the overlay |
| 2 | Suggestions are visible | Cancel suggestions |
| 3 | A turn is running (LLM streaming) | Cancel the turn |
| 4 | Editor is empty + double-press Esc within 500ms | Open session tree |
| — | Editor has text, single Esc | Nothing |

## Configuration

### Multiline mode

When enabled (default), Enter inserts a newline. When disabled, Enter always
submits. This is controlled by the `tui.editor.multiline` setting on hostd.

### Planned

| Setting | Description |
|---------|-------------|
| `tui.editor.maxLines` | Maximum visible lines before the editor scrolls |
| `tui.editor.autoResize` | Grow the editor height as the content expands |

### Key binding customization

All editor key bindings can be customized via `~/.piko/keybindings.json`
(global) and `.piko/keybindings.json` (project-level). Editor bindings use
the `tui.editor.*` and `tui.input.*` namespaces:

| Binding ID | Default |
|------------|---------|
| `tui.editor.cursorLeft` | Left |
| `tui.editor.cursorRight` | Right |
| `tui.editor.cursorLineStart` | Home |
| `tui.editor.cursorLineEnd` | End |
| `tui.editor.deleteCharBackward` | Backspace |
| `tui.editor.deleteCharForward` | Delete |
| `tui.input.newLine` | Ctrl+N |
| `tui.input.submit` | Enter |
| `tui.input.tab` | Tab |
| `tui.history.prev` | Ctrl+P |
| `tui.history.next` | Ctrl+E |

## Behavior when overlays are active

- **Partial overlay** (Model Selector, Command Palette, Settings, etc.): the
  Editor is replaced by the overlay. Keystrokes go to the overlay, not the
  Editor. Editor content is preserved and restored when the overlay closes.
- **Full overlay** (Session List, Help, Tree, Status): the Editor is replaced
  along with all middle slots. Same preservation on close.
- **Approval mode**: the Editor remains visible below the approval panel. The
  user can see but **not** type into the Editor until the approval is resolved
  (Enter to accept, Esc to decline). The cursor is hidden during this state.

## Reference blocks (pasted content)

When large text or images are pasted into the Editor, instead of inserting the
full content inline, the Editor inserts a **reference block** — a placeholder
that represents the pasted content as a single atomic unit.

### Behavior

| Paste type | Threshold | Placeholder format |
|------------|-----------|--------------------|
| Large text | > 10 lines or > 1000 characters | `[paste #N +123 lines]` or `[paste #N 1234 chars]` |
| Image | Any image paste | `[Image: filename.png]` |

When a paste qualifies as large, the full content is stored internally and a
compact placeholder replaces inline text. Normal small pastes are inserted as
regular text.

### Placeholder behavior

- **Atomic**: the placeholder is treated as a single unit — cursor movement,
  deletion, word-wrapping all treat it as one indivisible block. A single
  Backspace deletes the entire marker, not individual characters within it.
- **Readable**: the marker text is human-readable and compact, so the Editor
  doesn't become cluttered with large raw content.
- **Preserved on submit**: when the prompt is submitted, all markers are
  expanded to their original full content before being sent to the LLM.
- **Cleared**: after submission, all stored pastes are cleared along with the
  editor state.

### Image references

When an image is pasted from the clipboard, a similar reference block is
inserted showing the filename. The actual image is stored alongside the prompt
and attached to the message sent to the LLM (if the model supports vision).

## Non-goals

- Syntax highlighting (out of scope for a prompt input)
- Rich text / markdown editing
- Spell checking
- Undo/redo
- Vim/Emacs modal editing
- Mouse-based text selection
- Reference block expansion in the Editor itself (expansion happens on submit only)
