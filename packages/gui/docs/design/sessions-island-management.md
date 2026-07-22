# Sessions Island — Rename, Pin, and Delete

> Status: draft implementation design  
> Feature contract: [Sessions Island — Rename, Pin, and Delete](../features/sessions-island-management.md)  
> Parent: [GPUI Desktop Client Design](overview.md), [Overlay Stack](overlay-stack.md)

## 1. Problem

`SessionsIsland` today emits only open, create, open-directory, and focus
claims. Hostd already supports `SessionRename` and `SessionDelete`; Client Core
exposes `DeleteSession` but not rename. Pin has no host command anywhere.
Sidebar rows expose a single click target and a read-only message-count
accessory, so there is no entry point for housekeeping actions.

## 2. Goals and boundaries

| Layer | Owns |
|-------|------|
| `features/sessions` | Row context menu, rename transient body, wiring from menu → shell messages |
| `app/wiring` | `IslandMsg` dispatch → bridge intents, overlay open/close, `[gui]` pin persistence |
| `shell/overlay` | New `TransientKind` / `LocalConfirmKind` variants, Escape ordering |
| `projections` | MRU sort, global pin strip, dedupe, search filter application |
| `config` | `GuiSettings.pinned_session_ids`, `session_last_used_at_ms` |
| `piko-client-core` | `RenameSession` intent; delete list reconciliation |
| `piko-hostd` | Optional: cold-session path resolution for rename/delete (see §9) |

Shell does **not** gain session-specific forms; rename/delete bodies live under
`features/sessions` and mount through existing overlay surfaces.

## 3. Pin, MRU, and search (normative)

### 3.1 Product model

| Mechanism | Role | User-editable order? |
|-----------|------|----------------------|
| **Global Pinned band** | Short list of starred sessions across all cwd | **No** — MRU among pinned |
| **Directory groups** | Remaining unpinned sessions by cwd | **No** — group order = MRU of cwd |
| **Rows in group** | Unpinned sessions | **No** — MRU within cwd |
| **Search** | Substring filter over projected rows | N/A (hides non-matches) |

There is **no** pin-directory, **no** within-directory pin block, and **no**
manual reorder (drag, pin-sequence, alphabetical override).

### 3.2 Global pin strip

- **Membership**: `[gui].pinned-session-ids` (array treated as a set; serde
  dedupes on load).
- **Placement**: Flat `Pinned` section between search and directory tree.
- **Dedupe**: If `session_id ∈ pinned`, omit that session from its cwd group
  in the tree. Opening from either strip or group is identical.
- **Empty strip**: When no pins (or all filtered out), omit the section heading
  and band entirely.

Pin toggle does **not** write order into config — only membership.

### 3.3 MRU (single source of sort truth)

Persist last-use instants:

```toml
[gui]
session-last-used-at-ms = { "session_abc" = 1710000000000 }
```

Update rules (DesktopApp / bridge wiring):

| Event | Action |
|-------|--------|
| `OpenSession` intent sent for id | `now_ms` → map[id] |
| `CreateSession` success / new id known | `now_ms` → map[id] |
| Live session already open and user re-clicks same row | Optional no-op or bump (bump recommended) |

Do not update MRU on pin/unpin alone.

**Effective timestamp** for sort key:

```text
effective(session) =
  session_last_used_at_ms[id]
  ?? parse(modified_at from SessionSummary)
  ?? parse(created_at)
  ?? 0
```

**Pinned band order**: pinned ids sorted by `effective` **descending**;
tie-break `session_id` ascending.

**Unpinned in cwd group**: same sort, ids not in pinned set.

**Directory group order** (excluding transient Opening… group):

```text
group_rank(cwd) = max(effective(session) for unpinned sessions in cwd)
```

Sort groups by `group_rank` desc; tie-break normalized cwd key ascending.
Opening… group with pending target remains **first** regardless of MRU (existing
contract).

Never persist a separate “group order” or “pin order” list — recompute from MRU
map + host list on every derive.

### 3.4 Search filter

**State owner**: `SessionsIsland` entity (`filter: String`), window-local.

**Application**: After `derive_sidebar`, apply pure filter
`apply_sidebar_filter(vm, &filter) -> SidebarViewModel` (or filter while
flattening for render). Keeps derivation testable without GPUI.

Match predicate (case-insensitive contains):

- Row display label
- `SessionSummary.name`, `first_message`, `session_id`, `cwd`
- Parent group label (folder leaf / abbreviated path)

Effects:

- Pinned band: only matching pinned rows.
- Tree: drop non-matching sessions; drop empty groups; do not show pinned ids
  under groups (already deduped).

**Focus**: Search field uses key context `IslandSessionsSearch`. Island Tab
cycle includes the Sessions island when it is visible; Activate focuses the
Sessions chrome handle, not the search field. Clicking search focuses
`InputState` and emits ClaimFocus (chrome-only / Claimed — host must not steal).
Esc in search clears the filter (does not close overlays — lower priority than
OverlayHost).

### 3.5 Pin vs directory (explicit non-goals)

- No `pinned-cwd-keys`, no directory header pin menu in v1.
- MRU replaces alphabetical group sort from legacy sidebar derive — update
  [Workbench](../features/workbench.md) session entry accordingly.

### 3.6 Host / TUI

No `session.pin` or MRU in hostd. TUI resume panel keeps its own sort until
product aligns later.

## 4. Interaction architecture

```text
User right-clicks session row
  → SessionsIsland context menu
  → IslandMsg::RenameSession | DeleteSession | TogglePinSession
  → DesktopApp::dispatch_island_msg
       Rename  → OverlayHost.open_transient(SessionRename)
       Delete  → OverlayHost.open_local_confirm(DeleteSession)
       Pin     → patch pinned-session-ids + ConfigUpdate
  On rename/delete confirm:
       → ClientIntent::RenameSession | DeleteSession
  User opens session (sidebar click or other open path):
       → bump session-last-used-at-ms + ConfigUpdate
  → bridge.poll → derive_sidebar(SidebarPrefs) → apply filter (island-local)
  → SessionsIsland::apply
```

Primary click behavior stays unchanged. Context menu must call
`cx.stop_propagation()` so activate handlers do not fire.

### 4.1 Context menu (new GUI pattern)

There is no shared context-menu helper yet. Introduce a minimal feature-local
builder (or `shell/widgets/context_menu.rs` if a second consumer appears within
the same PR). Requirements:

- Trigger: `MouseButton::Right` and macOS Control+left (standard GPUI
  secondary-click if available; otherwise document right-click only for v1).
- Dismiss on outside click, Escape (only when no higher overlay is open), and
  after choosing an item.
- Menu width intrinsic; labels from `crate::t!(…)`.

Keep directory rows free of this menu.

### 4.2 Rename transient

Extend `TransientKind` (or parallel enum used by `OverlayHost`) with
`SessionRename { session_id, initial_name }`.

Body (`features/sessions/rename_dialog.rs`):

- Single-line `Input` (gpui_component), default focus on open.
- Primary **Save** / Enter; **Cancel** / Escape closes transient and restores
  island focus via existing `OverlayHost` focus restore.
- Validation: trim; reject empty; no-op if unchanged → close without intent.

Do not reuse Command Palette frame; reuse `OverlayPanelStyle::Transient` padding
from overlay-stack design.

### 4.3 Delete confirm

Extend `LocalConfirmKind` with `DeleteSession { session_id, display_name }`.

- Copy: title “Delete session?”, body includes display name and permanent loss
  warning; confirm uses danger styling (existing theme danger tokens).
- Confirm → `ClientIntent::DeleteSession`; cancel/Escape dismisses only the
  confirm.
- While confirm open, refuse opening Command Palette (existing HostPrompt /
  LocalConfirm rules).

Disable menu item (not hidden) when `SessionRowKind::PendingTarget` or when
deleting live session while `ClientState` reports blocking prompts on that
session.

## 5. IslandMsg and dispatch

Add shell-owned payloads to `shell/island/msg.rs`:

```rust
RenameSession { session_id: String },
DeleteSession { session_id: String },
TogglePinSession { session_id: String },
```

`app/wiring/island_dispatch.rs` matches these:

- **Rename / Delete**: resolve display name from latest `SidebarViewModel` or
  `ClientState.session_list` for confirm copy; open overlay.
- **TogglePin**: call `features/sessions/actions.rs::toggle_pin` on
  `DesktopApp` (patches gui config, updates local `GuiSettings`, re-derives
  sidebar).

Do not pass `SessionRow` or VM types into `IslandMsg`.

## 6. Client Core and protocol

### 6.1 Rename intent (new)

Add to `piko-client-core` `ClientIntent`:

```rust
RenameSession {
    session_id: SessionId,
    name: String,
}
```

Reducer sends `Command::SessionRename { command_id, session_id, name }` and
tracks `PendingOp::Rename { session_id }`.

On `CommandResponse::Ok(Empty)` for rename:

- Update matching entry in `state.session_list.sessions` (`name = Some(...)`).
- If live session id matches, update `live_session` name field when present on
  projection.

On `SessionReconciled` for that id (hostd always emits one today), merge name
into live snapshot if applicable — same pattern as open refresh.

Document in `docs/client-core-contract-baseline.md` when implemented.

### 6.2 Delete intent (existing)

Keep `DeleteSession { session_id }`. Extend host message handling:

After successful delete (`CommandResponse::Ok(Empty)` **or**
`SessionCleared`):

- Remove `session_id` from `state.session_list.sessions`.
- Existing `handle_session_cleared` already drops live projection when ids
  match.

Always enqueue `ClientIntent::DiscoverSessions { scope: All, cwd: None }` after
delete success so GUI stays aligned with storage-backed lists (hostd list is
authoritative).

### 6.3 Pin and MRU (GUI-only)

No protocol change. Pin membership and MRU map live in `[gui]` (§3.2–§3.3).
Toggle pin patches `pinned-session-ids` only. MRU bumps patch
`session-last-used-at-ms` on session activation.

## 7. Sidebar projection

Extend `SidebarViewModel`:

```rust
pub struct SidebarViewModel {
    pub pinned: Vec<SessionRow>,
    pub groups: Vec<SidebarGroup>,
}
```

`derive_sidebar(state, prefs)` where:

```rust
pub struct SidebarPrefs {
    pub pinned_session_ids: HashSet<String>,
    pub session_last_used_at_ms: HashMap<String, u64>,
}
```

Algorithm sketch:

1. Build cwd buckets from `session_list` (existing).
2. For each session, compute `effective` timestamp (§3.3).
3. Split pinned vs unpinned; assign pinned rows to `vm.pinned` (MRU sort).
4. For each cwd, fill group rows with unpinned only (MRU sort).
5. Sort groups by MRU rank (§3.3); keep Opening… first.
6. Pending target not in list: still inject Opening… group; pending row may
   appear in tree even if pinned (pin disabled for pending — do not add to
   pinned band while opening).

`SessionRow` fields:

```rust
pub is_pinned: bool,       // true in pinned band; false in groups
pub cwd_hint: String, // folder leaf for pinned rows only; rendered muted in detail slot
```

Rendering (`sidebar.rs`):

- **Search row** above scroll body (compact input).
- **Pinned** subheader + flat rows (`depth: 0`, no disclosure).
- Tree list for groups unchanged structurally; pinned ids absent from groups.
- Pinned rows: `ChromeIcon::Pin` leading; **required** `detail` = muted folder
  leaf (`folder_group_label(cwd)`), formatted as `· {leaf}` in Meta style via
  tool-window row `detail` slot (directory tree session rows omit this detail).
- Accessory remains message count.

Filter: `apply_sidebar_filter(&mut vm, query)` in `projections/` or
`features/sessions/filter.rs`.

Update projection tests: MRU group order, pinned dedupe, filter hides groups.

## 8. Focus and input

- Context menu: no island focus ring change.
- Rename transient: key context `SessionRenameDialog`; Enter/Escape handled in
  overlay layer before island fallback.
- After close, `OverlayHost` restores prior island focus (Sessions).
- Search field: key context `IslandSessionsSearch`; Esc clears filter when search
  focused (if no overlay).
- Pending open rows: rename/delete/pin disabled.

## 9. Hostd prerequisite (cold sessions)

Sidebar lists sessions from **storage summaries** (`SessionListScope::All`).
`apply_session_delete` and `apply_session_rename` today assume:

- `session_mut` succeeds (session loaded in `HostState`), and
- `session_paths` contains the disk path.

A session never opened in this hostd process may appear in the list but fail
rename/delete. **Recommendation:** small hostd change in `apply_session_*`:

1. Resolve `PathBuf` from `session_paths`, else from storage index /
   `SessionSummary.session_path` discovered at list time.
2. For rename without loaded state: load manifest via storage, append
   `session_info`, update summary name in list responses.
3. For delete: `remove_dir_all` resolved path even when not in memory; drop from
   `HostState` if present.

Track as a dependent PR or first slice gate; GUI can land with integration
tests against hostd that create+list+delete without open, and create+list+rename
without open.

## 10. Palette and commands (follow-up)

When sidebar grows keyboard selection, wire palette `session.rename` /
`session.delete` to selected row id. Until then, palette rows stay disabled as
today (`palette.disabled.needs_confirm` / needs-args copy).

## 11. i18n keys (sketch)

| Key | English |
|-----|---------|
| `island.sessions.search.placeholder` | Search sessions… |
| `island.sessions.section.pinned` | Pinned |
| `island.sessions.menu.open` | Open |
| `island.sessions.menu.rename` | Rename… |
| `island.sessions.menu.pin` | Pin |
| `island.sessions.menu.unpin` | Unpin |
| `island.sessions.menu.delete` | Delete… |
| `island.sessions.rename.title` | Rename session |
| `island.sessions.delete.title` | Delete session? |
| `island.sessions.delete.body` | “{name}” will be permanently deleted. |

## 12. Validation

```bash
cargo test -p piko-gui
cargo test -p piko-client-core
cargo test -p piko-hostd   # if §9 lands
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Unit tests:

- MRU sort: pinned band, within-group, and group order.
- Pinned session omitted from cwd group.
- Filter hides empty groups and non-matching pins.
- Pin list / MRU cleanup when session id absent.
- Delete intent removes list row + schedules discover (client-core).
- Menu disabled for `PendingTarget` row.

Manual M4: search narrows list; pin → row in global strip only; MRU moves row
after reopen; delete non-live vs live.

## 13. Implementation slices

| Slice | Deliverable |
|-------|-------------|
| A | Hostd cold path resolution (§9) + hostd tests |
| B | Client Core rename intent + delete list/discover refresh |
| C | `[gui]` pin + MRU prefs, `derive_sidebar` rewrite, search UI + filter |
| D | Context menu + overlays + `IslandMsg` wiring + feature tests |

Slices A+B can merge before UI; C+D need A for trustworthy rename/delete on
listed-but-never-opened sessions.

## 14. Tradeoffs

| Choice | Rationale |
|--------|-----------|
| Global pin + MRU | User mandate; pin = membership, MRU = sole order axis |
| Dedupe pinned from groups | Avoid duplicate rows and conflicting selection affordances |
| GUI-local MRU | Host list lacks reliable client use timestamps; no TUI coupling in v1 |
| Search in island | Matches resume-panel mental model; keeps palette for commands |
| Context menu vs hover ⋯ | Menu matches macOS Workbench expectations; keeps 32 px row height |
| Transient rename vs inline edit | Overlay reuses focus/Escape policy; inline edit fights tree row hit targets |
| Discover after delete | Host does not push `SessionListed` on delete; refresh avoids stale rows |

**Deferred:** host pin/MRU sync; directory pin; manual sort; keyboard-only
session management; multi-select.
