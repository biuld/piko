# GUI Overlay Stack Design

> Status: implemented design derived from the Feature Docs
> Feature contracts:
> [GUI Overlay Stack](../features/overlay-stack.md),
> [GUI Command Palette](../features/command-palette.md)
> Parent: [GPUI Desktop Client Design](overview.md)

## 1. Purpose

Make overlays a chrome-owned subsystem (alongside IslandPanel, TitleBar, and
StatusBar) with custom rendering and a single Escape / focus policy. Migrate
existing product modals off GPUI Component `open_dialog`, then land Command
Palette as the first Transient.

## 2. Responsibilities

| Owner | Owns | Does not own |
|---|---|---|
| `shell/overlay` | stack, priority, backdrop, panel geometry, Escape, focus trap/restore | host prompt semantics, catalog authority |
| `overlays/*` | approval / interaction form bodies | when to open, dismiss policy |
| `app/*` | Core projection sync, quit busy, palette action routing | modal geometry |
| Client Core | prompt queue facts | overlay UI state machine |
| hostd | `CommandCatalog` | GUI bindings and filtering |

## 3. Module layout

```text
packages/gui/src/shell/overlay/
├── mod.rs
├── host.rs       # OverlayHost state + open/close/Escape
├── kinds.rs      # HostPrompt | LocalConfirm | Transient
├── surface.rs    # backdrop + elevated panel
└── palette.rs    # Command Palette body

packages/gui/src/overlays/
├── prompt_coordinator.rs   # front projection (unchanged role)
├── approval_view.rs        # approval body (was dialog)
└── interaction_view.rs     # interaction body (was dialog)
```

`DesktopApp` holds an `OverlayHost` (plain state or Entity) and renders
`chrome::overlay` as a full-window absolute layer above Workbench content.
Product paths no longer call `window.open_dialog`. Sheet and notification Root
layers remain.

## 4. Priority and Escape

```text
HostPrompt > LocalConfirm > Transient > Sheet
```

Escape handler (`CloseTransientOverlay`):

1. if Command Palette submenu is open → pop one frame
2. else if Transient open → close Transient, restore focus
3. else if LocalConfirm open → cancel confirm, restore focus
4. else if sheet active → `close_sheet` + island focus restore
5. else no-op

Approval HostPrompt ignores Escape and backdrop. Interaction Cancel (button or
Escape-as-cancel when that is the HostPrompt dismiss path) sends
`UserInteractionResponse::Cancel` and waits for host/reconcile to clear.

## 5. Existing overlay wiring

| Entry | After |
|---|---|
| `sync_prompts` | `overlay.show_host_prompt` / `clear_host_prompt` via fingerprint |
| Approval / Interaction | HostPrompt surface + body builders |
| Busy quit | LocalConfirm; refused while HostPrompt active (or queued after) |
| Activity → prompt | still `sync_prompts`; UI shows Core front only |
| Sheets | GPUI sheet; Escape via OverlayHost |
| Toasts | notification layer only |

## 6. Command Palette

- Binding: `Cmd+Shift+P` → `OpenCommandPalette`
- Bridge sends `Command::CommandCatalogGet`; cache on DesktopApp / OverlayHost
- In-overlay navigation stack: root catalog → Models / Thinking submenus
- Filter applies to the current frame only
- Enter on Models/Thinking pushes a submenu; Enter on a leaf applies
  `SetModel` / `SetThinkingLevel` and closes the palette
- Does not enter Client Core state

### Action map

| Action | Behavior |
|---|---|
| Sessions / Agents / Tree | focus or open dock/sheet |
| Quit | existing quit path |
| ClearNotifications | clear toast / clearable Activity |
| NewSession | Open Directory / Sessions guidance |
| Commands | keep palette open |
| Models | push model list submenu → `SetModel` |
| Thinking | push thinking-level submenu → `SetThinkingLevel` |
| Help / Status / Settings / Login… | visible, disabled, deferred detail |
| Rename / Import / Export / Delete / Fork | visible, disabled, needs arguments |

## 7. Focus restore

OverlayHost records one focus episode. The app captures an archipelago-tagged
target (`Workbench(IslandId)` or `Settings(SettingsIslandId)`) on first open
and restores only that visible workspace when the final overlay closes. If the
active Archipelago changed unexpectedly, restore its current ring rather than
focusing a hidden entity.

## 8. Non-goals

- Sheet/Dock inside OverlayHost
- Composer slash / `@`
- Full Settings / Help / Login overlay bodies
- Client Core catalog or overlay SM
- TUI changes
