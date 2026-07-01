# Resume Session

## Overview

Resume Session is a full-screen panel for finding and opening an existing
conversation. It is used when the user wants to continue work from a previous
session instead of starting a new one.

The panel opens from `/resume`, `/sessions`, or `/session`, and from the
session resume key binding. Selecting a session closes the panel and restores
that session's timeline, editor state, model metadata, and session tree from
the saved snapshot.

## Layout

Resume Session is rendered as a full overlay that replaces the middle chat
area while keeping the BottomBar visible.

```
┌────────────────────────────────────────────────────────────┐
│ Resume Session (Current Folder)       Current | All  Recent │
│ Tab scope · Ctrl+N named · path                            │
│                                                            │
│ Search                                                     │
│                                                            │
│ › Refactor auth module                       18 messages 2h │
│   Fix TUI model selector                      7 messages 1d │
│   untitled                                   42 messages 3d │
│                                                            │
│ Enter resume · Esc close                                  │
└────────────────────────────────────────────────────────────┘
```

- The title names the active scope: Current Folder or All.
- The search field filters the visible sessions.
- Each row shows the session display title, activity metadata, and age.
- The current active session is visually marked.
- When the All scope is active, rows include enough path or cwd context to
  distinguish sessions from different projects.

## Behavior / interactions

### Opening

- `/resume` opens the panel.
- `/sessions` and `/session` open the same panel.
- The session resume key binding opens the panel from the Editor when no
  higher-priority overlay or suggestion list is active.

### Closing

- Esc closes the panel and returns to the previous chat view.
- `q` closes the panel.
- Closing the panel does not change the active session.

### Selection

| Key | Action |
|-----|--------|
| Up / Down | Move selection |
| Enter | Resume the selected session |
| Esc | Close panel |
| q | Close panel |
| Backspace | Delete one character from the search filter |

When a session is selected, hostd opens the selected session and emits the
restored snapshot. The TUI then closes the panel and renders the restored
session. If opening fails, the panel remains usable and an error is shown.

Selecting the already-active session is allowed but should behave as a no-op
refresh rather than creating a new session or losing editor state.

### Search

Typing printable characters filters the list. Filtering matches:

- session name
- first user-visible message
- session id
- cwd
- visible path text when paths are shown

When no sessions match, the panel shows an empty state instead of closing.

### Scope

| Key | Action |
|-----|--------|
| Tab | Toggle Current Folder / All |

Current Folder shows sessions whose cwd is the TUI's current working directory.
All shows sessions from every known project. The panel remembers the filter and
selection while switching scopes when possible.

When the current folder has no sessions, the empty state tells the user to
switch to All.

### Display title

Rows use the first available label in this order:

1. Explicit session name
2. First user-visible message
3. `untitled`

Named sessions are visually distinguishable from unnamed sessions.

### Optional list controls

The panel may expose additional controls when supported:

| Control | Behavior |
|---------|----------|
| Named filter | Toggle between all sessions and named sessions only |
| Path | Toggle full path or cwd display |
| Rename | Rename the selected session |
| Delete | Delete the selected session after confirmation |

Rename, delete, and sort-mode switching are not required for the first complete
Resume Session implementation. If delete is available, the currently active
session cannot be deleted from this panel.

## Configuration

All key bindings can be customized through the normal key binding mechanism.
The user-facing binding ids are:

| Binding ID | Default |
|------------|---------|
| `app.session.resume` | none |
| `app.session.toggleNamedFilter` | Ctrl+N |
| `app.session.togglePath` | none |
| `app.session.rename` | none |
| `app.session.delete` | none |

Resume Session does not persist panel-local UI state such as filter text,
selection, scope, or path visibility unless a future setting explicitly adds
that behavior.

## Non-goals

- Does not submit a prompt or start a turn when a session is resumed.
- Does not fork sessions from another project automatically.
- Does not hide hostd errors when a selected session cannot be opened.
- Does not render as a floating popup over the timeline.
- Does not require rename, delete, or threaded tree display for the initial
  usable version.
