# Editor

## Overview

The Editor is the primary text input panel where users compose prompts, type
slash commands, and trigger autocompletions. It sits in the lower portion of the
Chat layout, between the AgentPanel/NotificationRow and the BottomBar.

The Editor is the **default focus owner**: when no overlay panel is active, all
keystrokes flow here. Partial overlays (model selector, auth selector, etc.)
temporarily replace the Editor's slot — the Editor is not hidden behind a
panel, it is structurally absent during overlay sessions. When the overlay
closes, the Editor reappears with its content preserved.

## Layout

```
┌─────────────────────────────────────────────────────┐
│  (editor content, multi-line)                       │
└─────────────────────────────────────────────────────┘
```

- Minimum height 3 rows; by default the composer grows to six visible content
  rows before scrolling internally. A one-cell right gutter is always reserved;
  overflow paints the scrollbar in that gutter
- Top and bottom borders only, no left/right border
- **No elevated body fill** — the composer sits on the plane background. Pointer
  hover is visually inert; focus border and caret communicate input state.
- Border uses `prompt_border` / `prompt_border_active` when Chat-focused
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
longer input scrolls within the fixed-height region. The right-side gutter stays
reserved at all times and paints a scrollbar when the content exceeds the
visible rows. Mouse-wheel scrolling moves the editor viewport and its scrollbar
together; clicking the gutter jumps to the corresponding content position. A
newline key inserts a line break.

## Submission

### Submitting a prompt

**Enter** submits the current text and image content. Boundary whitespace is
trimmed from text. An image-only message is valid; empty text without an image
does nothing.

When the viewed agent is idle, Enter starts a turn (`ChatSubmit`). When that
agent is running, Enter steers the active turn (`QueueSteer`). **Alt+Enter**
always queues a follow-up (`ChatSubmit` / FollowUp). **Ctrl+Enter** steers only
and fails closed if the agent is idle. **Alt+↑** restores the last follow-up
this TUI queued. Details: [message-queue.md](./message-queue.md).

The submitted draft remains recoverable until hostd accepts the command. A
rejected submit restores the complete draft, including reference payloads, when
the composer is empty; if the user has already started a new draft, the failed
draft remains available through history without overwriting newer input. A
rejected follow-up must also disappear from the local queue projection.

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
| **Ctrl+E** | Next entry (go forward in history) while browsing; otherwise move cursor to the end of the line |

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

Typing `/` followed by one or more characters activates **slash suggestions**.
This is the sole command-discovery entry point and lists matching commands
from the merged local and host-advertised catalog.

### File path completion (`@`)

Typing `@` followed by characters activates the **File Browser** auto-completion.
- When the query is empty (just `@`), it lists files and folders at the top level of the current directory.
- When characters are typed, it performs a **global recursive fuzzy search** across all files in the project workspace (excluding standard build/version directories like `.git`, `node_modules`, `target`, etc.).
- Completed files are inserted into the editor as **placeholder blocks** (e.g. `[@src/main.rs]`), which are treated as a single cohesive unit: pressing `Backspace` once deletes the entire path at once. When submitted, the placeholder block expands back to `@src/main.rs` plain text.

### Completion UI

Suggestions appear in workspace `Region::Suggest` (directly above the Editor),
with bottom-only Minimal pane chrome. There is no provider header or selection
counter. Column outputs (command descriptions or file sizes) align consistently.

### Completion navigation and acceptance

| Key | Action |
|-----|--------|
| Tab / ↓ | Cycle selection downward (automatically updates the editor text with the selected option in real-time) |
| Shift+Tab / ↑ | Cycle selection upward (automatically updates the editor text with the selected option in real-time) |
| Enter | Accept the selected completion (for commands: immediately submits/executes; for files: locks path block and closes view) |
| Esc | Cancel suggestions, keep the current editor text, and return to normal editing |

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
| Shift+Enter | Insert newline |

### Submission and navigation

| Key | Action |
|-----|--------|
| Enter | Submit prompt (start, or steer if running) |
| Alt+Enter | Queue follow-up |
| Ctrl+Enter | Steer running turn only |
| Alt+↑ | Restore last queued follow-up |
| Ctrl+P | Previous history entry |
| Ctrl+E | Next history entry |
| F2 | Open session tree |
| F3 | Open model selector |

### Quit

| Key | Action |
|-----|--------|
| Ctrl+C | Clear the editor, or cancel the active turn |
| Ctrl+D | Quit the TUI |

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

When enabled (default), the effective terminal profile chooses the reliable
newline chord: enhanced terminals use Shift+Enter, while baseline terminals
use Ctrl+J. Enter always submits. When disabled, newline insertion is
unavailable. This is controlled by the `tui.editor.multiline` setting on
hostd.

### Sizing

| Setting | Description |
|---------|-------------|
| `tui.editor.maxLines` | Maximum visible lines before the editor scrolls |
| `tui.editor.autoResize` | Grow the editor height as the content expands |

### Key binding customization

Editor bindings use the host-owned `[tui.keybindings]` rule registry described
in [Keybindings and Command Routing](./keybindings.md). The TUI does not read
or migrate standalone `keybindings.json` files; those paths are intentionally
outside the current configuration contract.

## Behavior when overlays are active

- **Partial overlay** (Model Selector, Auth Selector, etc.): the
  Editor is replaced by the overlay. Keystrokes go to the overlay, not the
  Editor. Editor content is preserved and restored when the overlay closes.
- **Full overlay** (Session List, Tree, Usage): the Editor is replaced
  along with all middle slots. Same preservation on close.
- **Approval mode**: the Approval panel replaces the Editor as a partial
  overlay. The Editor draft is preserved and restored after the approval is
  resolved.

## Reference blocks (pasted content)

When large text or images are pasted into the Editor, instead of inserting the
full content inline, the Editor inserts a **reference block** — a placeholder
that represents the pasted content as a single atomic unit.

### Behavior

| Paste type | Threshold | Placeholder format |
|------------|-----------|--------------------|
| Large text | > 10 lines or > 1000 characters | `[paste #N +123 lines]` or `[paste #N 1234 chars]` |
| Image | Any image paste | `[Image #N: filename.png]` |
| Local image path | Whole bracketed paste is an absolute path to `png`, `jpg`/`jpeg`, `gif`, or `webp` | `[Image #N: filename.ext]` |

When a paste qualifies as large, the full content is stored internally and a
compact placeholder replaces inline text. Normal small pastes are inserted as
regular text.

### Placeholder behavior

- **Atomic**: the placeholder is treated as a single unit — cursor movement,
  deletion, word-wrapping all treat it as one indivisible block. A single
  Backspace deletes the entire marker, not individual characters within it.
- **Readable**: the marker text is human-readable and compact, so the Editor
  doesn't become cluttered with large raw content.
- **Preserved on submit**: text markers expand to text and image markers become
  ordered `ContentBlock::Image` values before the message is sent to hostd.
- **Cleared**: after submission, all stored pastes are cleared along with the
  editor state.
- **History-safe**: history stores the draft structure, not only the displayed
  marker text, so recalling an image or paste reference preserves its payload.

### Image references

Ctrl+V or Alt+V reads an image from the system clipboard, encodes the RGBA
pixels as PNG, and inserts a reference block. The actual base64 image is stored
in the editor draft and attached to the structured message. Text-only targets
reject the image before provider dispatch.

Dragging a local image into a terminal commonly produces one bracketed paste
containing its absolute path. The Editor recognizes supported image
extensions, reads the file through an application effect, and inserts the same
reference block instead of literal path text. A failed read leaves the draft
unchanged and shows an error. Relative paths, unsupported extensions, and
multi-line pastes keep normal text-paste behavior.

If the terminal reports a file-manager drag as individual key events instead,
the Editor recognizes the completed whole-draft absolute image path and
replaces it only after the file read succeeds. This covers Finder-to-Ghostty
dragging without treating a partial path as an attachment.

## Non-goals

- Syntax highlighting (out of scope for a prompt input)
- Rich text / markdown editing
- Spell checking
- Undo/redo
- Vim/Emacs modal editing
- Mouse-based text selection
- Reference block expansion in the Editor itself (expansion happens on submit only)
