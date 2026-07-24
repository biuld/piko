# Native Flat Context Menu Design

Status: Implemented

Feature contract: [Context Menu](../features/context-menu.md)

## 1. Problem

The current context-menu host embeds `gpui-component::menu::PopupMenu`. That
component applies `shadow_lg()` inside its own renderer: two shadows with a
largest vertical offset of 10 px. Chrome cannot replace that style through the
public API.

The existing adapter clips the upstream entity and paints another shadow around
it. This is a fragile paint workaround: GPUI shadows extend beyond layout
bounds, the nested clipping behavior is not a stable styling contract, and the
adapter cannot guarantee that the upstream tail is absent. The wrapper is also
already responsible for pointer anchoring, deferred paint, dismissal, and
focus, so keeping the upstream renderer no longer meaningfully reduces the
surface we own.

Copying or forking the full upstream popup is not appropriate. It includes
submenus, custom rows, links, checks, shortcut discovery, scrolling, and menu
bar integration that the product context menu does not need. The replacement
is therefore a deliberately flat component.

## 2. Decisions

1. Chrome owns a small context-menu component built directly with GPUI.
2. The component lives under `components::menu`, not `components::list`, so
   non-list controls can reuse it without depending on list-row types.
3. The public model supports action items and separators only. Action items
   carry label, enabled state, tone, and a callback.
4. Keyboard navigation, pointer selection, dismissal, focus restoration,
   geometry, and paint are component responsibilities.
5. Product code owns labels and maps callbacks to product messages or actions.
6. Context menus are anchored transient surfaces, not modal overlays. They do
   not enter `OverlayHost` or its priority stack.
7. The implementation does not use `gpui_component::menu`. Other
   `gpui-component` controls remain unaffected.
8. A secondary click is either the platform secondary mouse button or, on
   macOS, Control plus the primary button. Command-click remains a primary
   click.

## 3. Module boundary

```text
components/mod.rs
  init           shared component keybindings and private action setup
components/menu/
  mod.rs       public exports
  item.rs      immutable item specifications and tone
  navigation.rs pure enabled-item cursor movement
  registry.rs  one active menu and focus origin per window
  state.rs     menu Entity, focus, invocation, dismissal, and paint
  host.rs      secondary-click trigger, anchoring, and deferred layer
```

Each file should remain cohesive and below the repository's 500-line ceiling.
The old list-local context-menu adapter is removed after the first consumer
migrates.

## 4. Public API shape

The intended API is:

```rust
pub enum ContextMenuItemTone {
    Default,
    Destructive,
}

pub struct ContextMenuItem;

impl ContextMenuItem {
    pub fn action(
        label: impl Into<SharedString>,
        callback: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self;
    pub fn separator() -> Self;
    pub fn enabled(self, enabled: bool) -> Self;
    pub fn tone(self, tone: ContextMenuItemTone) -> Self;
}

pub struct ContextMenuSpec;

impl ContextMenuSpec {
    pub fn new(items: impl IntoIterator<Item = ContextMenuItem>) -> Self;
}

pub struct ContextMenuRequest {
    pub position: Point<Pixels>,
}

pub trait ContextMenuExt: ParentElement + Styled {
    fn context_menu(
        self,
        build: impl Fn(
            ContextMenuRequest,
            &mut Window,
            &mut App,
        ) -> ContextMenuSpec + 'static,
    ) -> ContextMenuHost<Self>;
}

// Registered by piko_chrome::components::init(cx).
```

The builder runs when the menu opens, so labels and enabled states reflect the
current presentation snapshot. It receives the secondary-click position in
window coordinates and application access so conditional consumers can query
their current state. Callback types stay encapsulated by constructors rather
than becoming public aliases.

An empty `ContextMenuSpec` is a conditional no-op: the host does not create a
menu Entity, move focus, or consume the secondary click beyond preventing an
underlying primary activation. This lets selectable documents expose Copy only
when the pointer is inside an existing selection.

`components::init` installs private actions and bindings under the
`PikoContextMenu` key context:

| Key | Action |
|---|---|
| Up | select previous enabled item |
| Down | select next enabled item |
| Enter | invoke selected item |
| Escape | dismiss |

The app calls this initializer once during startup. The action types do not
need to become part of the consumer-facing API.

## 5. State ownership

`ContextMenuHost<E>` wraps the trigger element and stores per-element GPUI
element state:

- open menu entity;
- pointer origin;
- dismissal subscription.

An application-global registry, keyed by GPUI `WindowId`, guarantees one active
menu per window. Its entry holds the active menu identity and the focus handle
that existed before the first menu opened. Replacing one open menu with another
dismisses the old entity without restoring focus and transfers the original
focus target to the new entity. Ordinary dismissal removes the matching entry
and restores that target.

The menu Entity owns:

- the immutable item vector;
- its own focus handle;
- the currently selected item index;
- measured bounds used for outside-click handling.

No menu state enters application entities, hostd, settings, or product view
models. Re-rendering the source row may rebuild the host, but the GPUI element
id preserves open state for that row while it remains mounted.

## 6. Event flow

```text
secondary click on trigger
  → stop propagation and prevent the row's primary activation
  → registry closes any previous menu and preserves the pre-menu focus target
  → build ContextMenuSpec and create menu Entity
  → place a deferred, window-sized interaction layer
  → anchor menu at pointer and clamp it to the safe viewport
  → focus menu

hover item
  → selected index = hovered enabled item

Up / Down
  → pure navigation skips separators and disabled items, wraps at ends

Enter or item click
  → take the callback from the selected item
  → close menu and restore prior focus
  → invoke callback exactly once

Escape or outside click
  → close menu and restore prior focus

window deactivation
  → close menu and restore the recorded logical focus target
```

Closing before callback execution prevents a callback that removes its source
row or opens an overlay from leaving a detached menu or restoring stale focus
over the new surface.

Only one context menu is visible in a window. A secondary click on a different
target replaces the old menu in the same event and preserves the original
focus restoration target. Modal overlays paint above context menus; an action
that opens one follows the normal close-then-open sequence.

## 7. Geometry and paint

Geometry is resolved from the pointer origin after menu measurement:

- safe window margin: 8 px;
- minimum width: 144 px;
- maximum width: min(320 px, viewport width minus safe margins);
- surface padding: 4 px;
- item height: 32 px;
- item horizontal padding: 12 px;
- surface and item radius: 8 px and 6 px respectively;
- one-pixel semantic border.

The default placement grows right and down from the pointer. If it would cross
an edge, the origin flips or clamps independently on each axis. Geometry is a
pure function so narrow-window and bottom-edge behavior can be unit tested.

Paint uses chrome tokens only:

| Part | Token / treatment |
|---|---|
| surface | `elevated` |
| normal text | `fg`, control typography |
| destructive text | danger role accent |
| disabled text | `muted_fg` with reduced opacity |
| hovered / keyboard-active row | `border` fill |
| outline | `border` |

Elevation is tonal rather than spatial. The `elevated` surface and `border`
outline provide separation in both palettes; no GPUI shadow is applied.

## 8. Consumer migration

The list/tree component changes its context-menu field from an upstream
`PopupMenu` builder to the chrome `ContextMenuSpec` builder. The Sessions
feature constructs four action items—Open, Rename, Pin/Unpin, and Delete—with
Delete marked destructive. Its existing product callbacks and confirmation
policy do not change.

After migration:

- remove the old `components/list/context_menu.rs` wrapper;
- remove `gpui_component::menu` imports from tree-list and Sessions code;
- keep `gpui-component` as a crate dependency for the controls still using it.

The same component later serves Timeline text selection with a conditional
single Copy item. That consumer does not broaden the flat item model.

## 9. Validation

### Pure tests

- navigation wraps and skips disabled items and separators;
- an all-disabled menu has no selection and Enter is a no-op;
- viewport placement clamps or flips at all four edges;
- the surface uses `elevated` and `border` without a shadow;
- destructive and disabled rows resolve the expected paint roles.

### Entity / integration tests

- secondary click does not invoke the underlying primary-click handler;
- clicking or pressing Enter invokes one callback and dismisses once;
- Escape and outside click invoke no callback;
- window deactivation dismisses the menu;
- focus is restored after every dismissal path;
- opening a second menu replaces the first and retains the original restore
  target.

### Visual review

- dark and light menu surfaces remain legible over an island;
- tonal elevation remains distinct without a dark halo or tail;
- the menu stays fully visible near every window edge;
- pointer hover and keyboard selection paint identically;
- Delete is recognizable as destructive without becoming a filled red row.

## 10. Deferred scope

If a real consumer later requires submenus, scrolling, custom elements,
checks, or shortcut labels, that need must be designed as a separate extension.
It should not broaden the initial component speculatively. Native platform menu
integration can also be reconsidered independently if GPUI exposes a suitable
cross-platform API.
