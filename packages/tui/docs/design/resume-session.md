# Resume Session Design

## Selected Feature

This design implements the `Resume Session` feature contract in
`packages/tui/docs/features/resume-session.md`.

The user-visible feature is a full-screen panel for finding and opening an
existing session from the current project or from all known projects. The panel
is opened by `/resume` and the session resume key binding. Selecting a row asks hostd to open that session and then rebuilds the
TUI from the returned snapshot.

## Responsibilities

hostd owns:

- session discovery
- current-folder vs all-project scope
- resolving a selected session to a persisted session
- opening the selected session and returning a snapshot
- session rename/delete when those controls are implemented
- persisted session metadata used for display

The TUI owns:

- panel-local filter text, selection, scope, named-only filter, and path
  visibility
- rendering the full-screen Resume Session panel
- routing keys while the panel has focus
- sending session list/open commands to hostd
- applying the returned `SessionOpened` snapshot through existing event flow

The protocol owns:

- serializable command and event shapes shared by TUI and hostd
- `SessionSummary` fields required to render the list without TUI filesystem
  reads

## Contract Requirements

- `SessionSummary` exposes the fields required to render the resume list:
  session id, cwd, seq, name, first message, message count, modified time, and
  storage path metadata.
- `SessionList` supports both current-folder and all-project scopes.
- `SessionOpen` accepts enough identity to open a selected session regardless of
  the process current cwd.
- Resume has a distinct command route and key route from tree navigation.

## Target Data Flow

```text
User opens /resume
        |
        v
TUI sends SessionList { scope: CurrentFolder, cwd }
        |
        v
hostd scans session storage and emits SessionListed
        |
        v
TUI renders Resume Session panel
        |
        v
User changes filter/scope locally or confirms a selected row
        |
        v
TUI sends SessionOpen { session_id, session_path? }
        |
        v
hostd opens selected session globally and emits SessionOpened
        |
        v
TUI clears overlay and applies snapshot
```

Scope changes can either request a fresh list from hostd or use cached results
if both scopes have already been loaded. The first implementation should prefer
fresh hostd requests for correctness and keep caching as a local optimization.

## Protocol Shape

### SessionList

`SessionList` needs enough information to ask for either current-folder sessions
or all sessions.

```rust
pub enum SessionListScope {
    CurrentFolder,
    All,
}

Command::SessionList {
    command_id: CommandId,
    scope: SessionListScope,
    cwd: Option<String>,
}
```

Compatibility option: keep the existing no-field command shape temporarily and
treat it as `All`. The TUI should move to the scoped form for Resume Session.

### SessionOpen

Opening must resolve the exact selected session, not re-run a current-cwd-only
prefix lookup that may pick the wrong session or fail.

```rust
Command::SessionOpen {
    command_id: CommandId,
    session_id: SessionId,
    session_path: Option<String>,
}
```

`session_path` is optional for compatibility, but when it is present hostd
should open that exact persisted session. For stored piko sessions,
`session_path` is the persisted session directory, not the main JSONL file.

If exposing a path in the command is undesirable long term, the alternative is
for hostd to maintain an internal session id to path index after listing and
open by globally unique session id. The path-backed command is simpler and
matches the selected-row contract more directly.

### SessionSummary

`SessionSummary` should become the render-ready summary shape for the selector.

```rust
pub struct SessionSummary {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub name: Option<String>,
    pub first_message: Option<String>,
    pub message_count: u64,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub session_path: Option<String>,
    pub parent_session_path: Option<String>,
}
```

Rules:

- `first_message` is the first user-visible user message in the main branch when
  available.
- `message_count` counts user, assistant, and tool-result message entries.
- `modified_at` comes from the newest session entry timestamp or filesystem
  metadata when entry timestamps are unavailable.
- `session_path` is present only for persisted sessions.
- `parent_session_path` supports future threaded/tree ordering in the panel.

## hostd Design

### Listing

hostd should expose two listing modes through the same command:

- Current Folder: list sessions under the encoded current cwd.
- All: list every session under the configured session root.

The storage repository already has `list(Some(cwd))` and `list(None)`. The main
change is to pass the requested scope instead of always calling global
summaries.

While building summaries, hostd should parse persisted session state once and
derive the render fields from the loaded state and persisted path. The TUI
should not inspect session files directly.

### Opening

Opening a selected row must use one of these exact resolution paths:

1. If `session_path` is provided, load that session directory.
2. Otherwise, search all known sessions by exact id.
3. As a fallback for manual commands, allow id-prefix matching only when it is
   unambiguous.

Opening a session from another cwd changes the active session cwd to that
session's cwd. It does not fork the session.

### Errors

hostd should return protocol errors for:

- selected session path no longer exists
- selected path is not a valid session
- session id prefix is ambiguous
- session cannot be read

The TUI surfaces the error and leaves the Resume Session panel open.

## TUI Design

### Slot and Layout Integration

Resume Session is a full overlay in the existing flat slot system. It should not
introduce a new layout mode, nested area, centered popup, or absolute/floating
surface.

The integration path is:

```text
AppMode::Sessions
        |
        v
Placement::Full
        |
        v
LayoutMode::FullOverlay { mode: AppMode::Sessions }
        |
        v
build_full()
        |
        +--> Slot A: Resume Session panel, Constraint::Fill(1)
        +--> Slot E: BottomBar, Constraint::Length(1)
```

This means that while Resume Session is open:

- Slot A is replaced by the Resume Session panel.
- Slot B AgentPanel is absent.
- Slot C NotificationRow is absent.
- Slot D' Suggestions is absent.
- Slot D Editor is absent.
- Slot E BottomBar remains visible.

`build_constraints()` must remain pure and should not gain Resume Session
specific branches. It already supports this feature through
`LayoutMode::FullOverlay`. The only layout requirement is that
`AppMode::Sessions.placement()` remains `Some(Placement::Full)`.

Rendering stays in the existing full-panel dispatch:

```rust
fn render_full_panel(frame, app, area, mode) {
    match mode {
        AppMode::Sessions => app.sessions.render(...),
        ...
    }
}
```

The panel receives the full Slot A rectangle and must fit all visible content
inside that area. Any header, filter row, list body, empty state, or status line
is internal to the Session panel's own render method; none of these internal
regions are layout slots.

Focus also follows the existing LIFO stack:

- Opening Resume Session pushes `AppMode::Sessions`.
- Closing it pops back to the previous mode, normally `AppMode::Chat`.
- A successful `SessionOpened` clears focus back to Chat after applying the
  restored snapshot.

Because all non-Chat modes are currently capture surfaces, keys handled by
Resume Session do not pass through to the Editor. This matches the feature
contract: typing filters sessions instead of editing the prompt.

### Panel Model

Reuse the existing `AppMode::Sessions` full overlay, but rename presentation
copy to Resume Session. Implementation can keep the existing module name until
the feature stabilizes.

Panel-local state:

```rust
pub enum SessionScope {
    CurrentFolder,
    All,
}

pub struct SessionList {
    list: FilterableList<SessionSummary>,
    scope: SessionScope,
    named_only: bool,
    show_path: bool,
    loading: bool,
    error: Option<String>,
}
```

The existing `AppState.filter_text` can remain the shared overlay filter for
the first implementation. If session-specific filter state becomes necessary,
move it into `SessionList` so switching away from the panel cannot leak filter
text into models/settings/tree.

### Rendering

The panel should render rows as one-line session summaries:

- left side: selection marker, optional tree indentation, display title
- right side: optional cwd/path, message count, age
- active session: accent style
- named session: distinct style
- empty state: scope-aware message

Display title order:

1. `name`
2. `first_message`
3. `untitled`

`FilterableList` can continue to provide selection mechanics, but the current
generic renderer is too limited for the desired row layout.

To achieve robust vertical column alignment and handle wide CJK characters (like
Chinese) correctly, the panel renders the filtered items using a `Table` widget
(`ratatui::widgets::Table`) instead of a `List` widget.

The columns are laid out dynamically:
1. Marker & Title: `Constraint::Fill(1)` (left-aligned)
2. Optional Path/Cwd (when `show_path` or scope is `All`): `Constraint::Percentage(30)` (left-aligned, muted)
3. Message count: `Constraint::Length(12)` (right-aligned, muted)
4. Age: `Constraint::Length(8)` (right-aligned, muted)
5. Active status: `Constraint::Length(8)` (right-aligned, accented)

Each row in the table corresponds to a session. This keeps the shared component
stable and ensures clean scaling for all terminal widths without manual cell
padding calculations.

### Filtering and Sorting

Filtering should match:

- name
- first message
- session id
- cwd
- path when available

Recent sorting is the first required mode. It sorts by `modified_at` descending,
then `created_at`, then session id.

Threaded and relevance sorting can be added later. Sort controls must remain
hidden until they change behavior.

### Input Routing

In `AppMode::Sessions`:

- printable characters append to the session filter
- Backspace removes one filter character
- Up/Down move selection
- Enter opens the selected session
- Esc and `q` close the panel
- Tab toggles scope and requests the corresponding list from hostd
- Ctrl+N toggles named-only if implemented
- path/rename/delete key actions are routed only when implemented

From `AppMode::Chat`, the session resume key action must route to
`Action::RequestSessions`, while the tree key action routes to `Action::OpenTree`.

### Opening Selection

The selected item should provide both `session_id` and `session_path`. Dispatch
sends both when available:

```rust
Command::SessionOpen {
    command_id,
    session_id,
    session_path,
}
```

On successful send, the TUI may close the panel optimistically or keep it until
`SessionOpened`. The safer behavior is:

- set loading/status to `opening session`
- keep panel state available
- close only when `SessionOpened` is applied
- keep panel open on error

The existing `SessionOpened` event application already calls `apply_snapshot`,
sets `session_id`, and rebuilds active UI state. The event handler should clear
the sessions overlay after a successful session open.

## Feature Phasing

### Phase 1: Correct Resume

- Add scoped `SessionList`.
- Expand `SessionSummary`.
- Make `SessionOpen` open the selected global session correctly.
- Render display title, cwd, message count, age, active marker.
- Fix chat input routing for session resume vs tree.
- Keep recent sorting only.

### Phase 2: pi-mono Parity Controls

- Add Current/All caching and load progress.
- Add named-only filter.
- Add path visibility toggle.
- Add explicit sort cycling.
- Add first-pass threaded ordering from `parent_session_path`.

### Phase 3: Mutations

- Add rename selected session.
- Add delete selected session with confirmation.
- Prevent deleting the active session.
- Refresh list after mutation.

## Validation Plan

Focused validation:

- unit tests for scoped listing and global session open behavior in hostd
- unit tests for `SessionSummary` derivation from session entries
- TUI tests for selection/filtering and active-session marking where current
  test infrastructure supports it
- manual TUI smoke test for `/resume`, scope toggle, filter, open same cwd, and
  open different cwd

Commands:

- `cargo fmt --all`
- `cargo test -p piko-protocol`
- `cargo test -p hostd`
- `cargo test -p tui`
- `cargo clippy --workspace --all-targets -- -D warnings`

Use `cargo test --workspace` if protocol or storage changes reveal cross-crate
regressions beyond the focused tests.
