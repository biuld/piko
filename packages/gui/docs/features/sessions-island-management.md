# Sessions Island — Pin, Search, Rename, and Delete

> Status: draft product contract (not yet implemented)

## Overview

The Sessions island lists every conversation grouped by working directory. Users
need quick access to important sessions (global pins), fast lookup (search), and
housekeeping (rename, delete). **Sort order is always automatic** — most
recently used (MRU) — and cannot be changed manually.

## Layout

Below the island header (title and Open Directory), the body has three bands:

1. **Search** — compact filter field (always visible when the list is non-empty).
2. **Pinned** — global strip of pinned sessions (flat list, not grouped by cwd).
3. **By directory** — expandable cwd groups; sessions inside each group.

Session rows keep tool-window geometry: leading icon, truncated title, message
count in the accessory rail. Pinned rows in the global strip show a pin leading
icon and a **muted project hint** after the title (same leaf name as the
directory group header, e.g. `Release prep · myapp`).

```
Sessions                                    [↗]
Search sessions…
Pinned
  📌 Release prep · myapp                   12
  📌 Auth spike · other                      4
▾ myapp                                  [+]
  💬 Fix redirect                            3
▾ other                                  [+]
  💬 untitled                                0
```

Pinned sessions **do not repeat** under their directory groups. Unpin returns
a session to its cwd group only.

Secondary interaction (right-click or control-click) opens a context menu.
Rename uses a centered transient dialog; delete uses a destructive confirm.

## Behavior and interactions

### Search

- Typing filters the **current** sidebar projection (Pinned + directory tree).
- Matches are case-insensitive substring hits on: display title, explicit name,
  first user-visible message, session id, cwd, and directory group label.
- Non-matching sessions are hidden. Directory headers hide when they would have
  no visible sessions after filtering (including unpinned-only rows).
- The Pinned band hides when no pinned session matches the filter.
- Clearing the filter restores the full MRU-sorted view.
- Filter text is **window-local** and is not persisted.
- Empty filter with zero total sessions still uses the existing empty state
  (search hidden or disabled until at least one session exists).

### Pin

- **Pin session** only — there is no pin directory / pin folder.
- Pinned sessions appear **only** in the global **Pinned** band at the top.
- Pin and Unpin toggle membership immediately (no confirm).
- Pin does not change hostd authority, cwd, or live session.
- Pinned membership persists under `[gui]`.
- Order within the Pinned band follows **MRU among pinned sessions** (see
  Sorting) — not pin-click order and not user-editable.
- Delete or list refresh removes stale pin ids automatically.

### Sorting (MRU only, not user-editable)

All ordering is derived from **last used** timestamps maintained by the GUI.
There is **no** drag-and-drop, no manual reorder, and no alphabetical sort.

**Last used** updates when the user opens a session from the sidebar (or any
client path that activates that session in the Workbench). Creating a new
session counts as use for that session.

| Region | Order rule |
|--------|------------|
| **Pinned band** | Pinned sessions, most recently used first |
| **Directory groups** | Groups ordered by the most recent use of any session in that cwd (Opening… transient group stays first when present) |
| **Sessions inside a group** | Unpinned sessions in that cwd, most recently used first |

Sessions never opened in this client fall back to host `modified_at`, then
`created_at`, then stable id tie-break — still without user override.

### Context menu

On session rows (Pinned band and directory rows; not directory headers).

| Item | Effect |
|------|--------|
| Open | Same as primary click |
| Rename… | Rename dialog with current display title |
| Pin / Unpin | Toggle global pin membership |
| Delete… | Confirm, then permanent delete |

Directory headers: expand/collapse and New Session only.

### Rename

Same rules as before: non-empty trimmed name, Enter commits, Esc cancels, host
authoritative label refresh.

### Delete

Same rules as before: confirm with display title; live vs non-live behavior;
disabled for pending/opening targets and blocking prompts on live delete.

### Command palette

Rename/delete palette wiring remains deferred until a explicit sidebar target
exists. Search is sidebar-local, not the Command Palette.

## Configuration

- **`[gui].pinned-session-ids`** — set of pinned session ids (membership only;
  order comes from MRU, not this list).
- **`[gui].session-last-used-at-ms`** — map of session id → unix ms (GUI-owned
  MRU facts).
- **Search filter** — not persisted.
- Chrome copy in `gui.yml` for search placeholder, Pinned section label, menus,
  rename, and delete.

## Non-goals

- Pin directory or reorder directory groups manually.
- Drag-and-drop or any user-controlled sort.
- Alphabetical session or folder ordering (replaced by MRU for this island).
- Duplicate pinned rows under cwd groups.
- Cross-device sync of pins or MRU (local `[gui]` only).
- Bulk select, fork/clone/import/export from the sidebar menu.
- Host-level pin or MRU shared with the TUI in v1.
