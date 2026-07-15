# Editor Design

## Purpose

The Editor is the default text-input surface in the chat layout. It owns prompt
drafting, slash-command entry, completion interaction, prompt history, and paste
reference blocks.

The feature contract is defined in `packages/tui/docs/features/editor.md`. This
document describes a clean implementation design for that contract, independent
of the current code shape.

## Responsibilities

The Editor owns:

- the editable prompt draft
- cursor movement within the draft
- prompt submission preparation
- local prompt history
- completion trigger detection and acceptance
- paste reference placeholders and expansion
- rendering measurements for layout and terminal cursor placement

The Editor does not own:

- slash command execution
- available slash command definitions
- hostd command transport
- file-system completion policy beyond asking a completion provider
- model attachment capability decisions
- overlay focus ownership

## Architecture

```text
Raw terminal input
        |
        v
InputRouter
        |
        v
EditorController
        |
        +--> EditorBuffer
        +--> EditorHistory
        +--> CompletionSession
        +--> ReferenceStore
        |
        v
EditorRenderModel
```

### EditorController

`EditorController` is the command-facing boundary. It receives high-level
actions from input routing and mutates editor-owned state.

Core methods:

```rust
pub struct EditorController {
    buffer: EditorBuffer,
    history: EditorHistory,
    completions: CompletionSession,
    references: ReferenceStore,
    config: EditorConfig,
}

impl EditorController {
    pub fn handle_edit(&mut self, edit: EditCommand);
    pub fn handle_navigation(&mut self, nav: NavigationCommand);
    pub fn handle_history(&mut self, nav: HistoryCommand);
    pub fn handle_paste(&mut self, paste: PastePayload);
    pub fn refresh_completions(&mut self, providers: &CompletionProviders);
    pub fn accept_completion(&mut self, mode: CompletionAcceptMode) -> Option<SubmitRequest>;
    pub fn submit(&mut self) -> Option<SubmitRequest>;
    pub fn cancel_suggestions(&mut self);
    pub fn render_model(&self, width: u16) -> EditorRenderModel;
}
```

The controller returns `SubmitRequest` rather than sending to hostd directly.
The app layer decides whether the request is a slash command or prompt.

### EditorBuffer

`EditorBuffer` is the authoritative editable document. It stores text plus
inline reference atoms.

```rust
pub struct EditorBuffer {
    atoms: Vec<EditorAtom>,
    cursor: CursorPosition,
}

pub enum EditorAtom {
    Text(String),
    Reference(ReferenceId),
}
```

The buffer exposes text-like operations while preserving reference block
atomicity:

- insert character
- insert string
- insert newline
- replace byte or atom range
- delete backward
- delete forward
- move left/right within current line
- move to line start/end
- produce display text with placeholders
- produce expanded submission text

Reference atoms are indivisible. Cursor movement can stop before or after a
reference, but never inside it. Backspace/Delete remove the whole reference atom
in one operation.

### Cursor Model

The cursor is logical, not terminal-specific.

```rust
pub struct CursorPosition {
    atom_index: usize,
    offset: AtomOffset,
}

pub enum AtomOffset {
    TextByte(usize),
    BeforeReference,
    AfterReference,
}
```

All offsets must be valid UTF-8 boundaries. Rendering converts the logical
cursor into a visible row/column after wrapping and scrolling decisions.

Cursor movement is bounded by line boundaries:

- Left at column 0 stops.
- Right at end-of-line stops.
- Left/Right do not wrap to adjacent lines.
- Home/End operate on the current logical line.

### EditorHistory

History is local TUI state and stores submitted prompt text after reference
expansion.

```rust
pub struct EditorHistory {
    entries: VecDeque<String>,
    browse_index: Option<usize>,
    draft_before_browse: Option<EditorBuffer>,
    capacity: usize,
}
```

Rules:

- Capacity is 100 entries.
- Consecutive duplicate submissions are ignored.
- `Ctrl+P` enters history browsing from any draft state.
- First history browse stores the current draft in `draft_before_browse`.
- Repeated `Ctrl+P` moves toward older entries and wraps at the oldest entry.
- `Ctrl+E` moves toward newer entries.
- Moving past the newest entry restores the saved draft, or an empty draft if
  no saved draft exists.
- Any edit or cursor movement exits history browsing and keeps the visible text
  as the new live draft.

History entries should not persist to session JSONL. They are an editor affordance,
not conversation state.

## Completion Design

Completions are triggered automatically from the current cursor context.

```rust
pub enum CompletionTrigger {
    SlashCommand {
        range: BufferRange,
        prefix: String,
    },
    FilePath {
        range: BufferRange,
        typed: String,
    },
}

pub struct CompletionItem {
    pub label: String,
    pub detail: String,
    pub replacement: String,
    pub replace_range: BufferRange,
    pub kind: CompletionKind,
}
```

### Providers

The Editor depends on provider traits, not concrete command or file systems.

```rust
pub trait SlashCommandCompletionProvider {
    fn complete_slash(&self, prefix: &str) -> Vec<CompletionItem>;
}

pub trait FileCompletionProvider {
    fn complete_file(&self, cwd: &Path, typed: &str) -> Vec<CompletionItem>;
}
```

Slash command definitions should come from a shared slash-command registry used
by:

- slash command parser
- completion provider
- command palette/help display

This avoids divergent hard-coded command lists.

### Trigger Rules

Slash command completion:

- active only when the draft starts with `/`
- matches the command token up to the first whitespace
- ignores arguments after the command token
- uses the registry's command name and description

File path completion:

- active when the cursor is inside an `@...` token
- resolves entries from the current working directory
- sorts results alphabetically
- appends `/` to directory replacements and labels

### Session Behavior

`CompletionSession` stores visible items, selected index, and trigger metadata.

```rust
pub struct CompletionSession {
    pub items: Vec<CompletionItem>,
    pub selected: usize,
    pub trigger: Option<CompletionTrigger>,
}
```

Rules:

- Suggestions are visible whenever a trigger is active, even if zero items
  match. Zero matches render an empty state.
- Typing updates the list in real time.
- Up/Down move selection when suggestions are visible.
- Tab accepts selected completion and keeps suggestions open.
- Enter accepts selected completion and submits immediately.
- Esc cancels suggestions without changing the draft.

Completion acceptance must replace `replace_range`; it must not insert at the
cursor blindly.

## Submission Design

Submission produces:

```rust
pub struct SubmitRequest {
    pub text: String,
    pub references: Vec<ResolvedReference>,
}
```

Flow:

1. Expand reference atoms into their original content.
2. Trim leading/trailing whitespace from the expanded text.
3. If empty, return `None`.
4. Push expanded text into history.
5. Clear buffer, references, completions, and history browse state.
6. Return `SubmitRequest`.

The app layer then handles:

- slash command interception if `text.starts_with('/')`
- known slash command execution
- unknown slash command error notification
- normal prompt submission to hostd

If the app rejects submission because the slash command is unknown, it should
restore the pre-submit buffer rather than reconstructing text through insertion.

## Paste Reference Blocks

Paste handling turns large or non-text content into compact inline references.

```rust
pub enum PastePayload {
    Text(String),
    Image {
        filename: String,
        bytes: Vec<u8>,
        mime_type: String,
    },
}

pub struct ReferenceStore {
    next_id: u32,
    items: HashMap<ReferenceId, ReferencePayload>,
}
```

Thresholds:

| Type | Threshold |
|---|---|
| Large text | more than 10 lines or more than 1000 characters |
| Image | any image paste |

Placeholders:

- large text by lines: `[paste #N +123 lines]`
- large text by chars: `[paste #N 1234 chars]`
- image: `[Image: filename.png]`

Rules:

- Small text paste inserts normal text.
- Large text paste inserts one reference atom.
- Image paste inserts one reference atom and stores image bytes.
- Reference placeholders are readable display labels.
- Reference atoms are atomic for cursor movement and deletion.
- References expand only during submission.
- References are cleared after successful submission.

Image references should remain editor-local until protocol support exists for
vision attachments. Once hostd/protocol supports attachments, `SubmitRequest`
can carry resolved image references alongside text.

## Layout And Rendering

The Editor participates in the flat TUI layout as a normal row.

```rust
pub struct EditorRenderModel {
    pub display_lines: Vec<String>,
    pub visible_start_line: usize,
    pub visible_height: u16,
    pub cursor: Option<VisibleCursor>,
    pub suggestions: Option<CompletionRenderModel>,
}
```

The layout engine asks the editor for height:

```rust
pub fn visible_height(&self, width: u16) -> u16;
```

Rules:

- Minimum height is 3 rows: one content row plus top and bottom border.
- Borders are top and bottom only.
- Border color is the theme's muted chrome color in normal mode.
- Content height is one row unless auto-resize is enabled.
- With auto-resize enabled, content height grows up to `max_lines`.
- Longer content scrolls internally.
- Terminal cursor is shown only when the Editor accepts input.
- Cursor position is clamped within the visible content area.

Completion suggestions are rendered in a dedicated layout slot above the Editor.
They are not floating overlays.

Suggestion height:

- title row plus list rows
- at most 8 total rows
- shows empty state when trigger is active and no items match

## Focus And Input Priority

Input priority:

1. Active overlay handles keys.
2. Suggestions handle Esc, Up, Down, Tab, Enter.
3. Running turn handles Esc cancellation.
4. Editor handles text editing, history, submission, and shortcuts.

Editor visibility and input behavior:

- Chat mode: visible and accepts input.
- Partial overlay: structurally replaced; does not accept input.
- Full overlay: structurally replaced; does not accept input.
- Approval mode: visible below approval panel but read-only; cursor hidden.

Esc behavior:

1. Close active overlay.
2. Cancel visible suggestions.
3. Cancel active turn.
4. If editor is empty and Esc is pressed twice within 500ms, open session tree.
5. If editor has text, single Esc does nothing.

## Configuration

Editor settings live under hostd's opaque `tui` namespace.

```toml
[tui.editor]
multiline = true
maxLines = 1
autoResize = false
largePasteLines = 10
largePasteChars = 1000
historyLimit = 100
```

Recommended Rust model:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "default_multiline")]
    pub multiline: bool,
    #[serde(default = "default_max_lines")]
    pub max_lines: u16,
    #[serde(default)]
    pub auto_resize: bool,
    #[serde(default = "default_large_paste_lines")]
    pub large_paste_lines: usize,
    #[serde(default = "default_large_paste_chars")]
    pub large_paste_chars: usize,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
}
```

Keybindings stay in `~/.piko/keybindings.json` and `.piko/keybindings.json`.
Binding IDs:

| Binding ID | Default |
|---|---|
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

`multiline` controls the semantic action assigned to Enter when no custom
binding overrides it:

- `multiline = true`: Enter inserts newline; submit must use the configured
  submit binding.
- `multiline = false`: Enter submits.

## Protocol Boundary

For text-only prompts, the app can continue to send:

```rust
Command::ChatSubmit {
    session_id,
    target_agent_instance_id,
    text,
    ...
}
```

For image references, protocol should later grow a structured message command:

```rust
Command::ChatSubmitMessage {
    session_id,
    target_agent_instance_id,
    content: Vec<ContentBlock>,
    ...
}
```

Until then, image paste can be represented in the editor UI but should not be
silently submitted as a real vision attachment.

## Test Plan

Unit tests:

- character insertion, deletion, and cursor boundaries
- line start/end movement
- range replacement for completion acceptance
- slash trigger detection
- file trigger detection
- history browse, wrap, draft restore, dedup, capacity
- large text threshold and placeholder formatting
- reference atom cursor/delete behavior
- submit expansion and cleanup

App/input tests:

- suggestions intercept Up/Down/Tab/Enter
- Esc priority chain
- unknown slash command restores rejected draft
- approval mode keeps editor visible and read-only
- overlay replacement preserves draft

Render/layout tests:

- editor height with fixed one-line mode
- editor height with auto-resize and max lines
- suggestion slot height capped at 8 rows
- cursor is hidden outside editable chat mode

## Migration Plan

1. Add `EditorConfig` under `TuiConfig`.
2. Introduce `EditorBuffer` and implement range replacement first.
3. Move history into `EditorHistory`.
4. Introduce `CompletionTrigger` and provider traits.
5. Extract a shared slash command registry.
6. Add paste text handling and `ReferenceStore`.
7. Add atomic reference behavior.
8. Add render model and dynamic height.
9. Extend protocol for structured content if image submission is required.
