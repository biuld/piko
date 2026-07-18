# GPUI Dependency Pins

> Phase 1 spike result — compatibility verified on macOS (aarch64).

## Pinned Versions

| Crate | Source | Version/Rev | Notes |
|---|---|---|---|
| `gpui` | crates.io | `=0.2.2` | Immutable registry release; equivalent to Zed `main` circa 2025-10-22 |
| `gpui-component` | crates.io | `=0.5.1` | Published 2026-02-05; depends on gpui ^0.2.2 |

Registry releases are immutable — the `=` version requirement in `Cargo.toml`
provides the same guarantees as a git SHA pin but with faster resolution and
smaller lockfile.

### Features enabled

```toml
gpui = { version = "=0.2.2", default-features = false, features = ["font-kit", "runtime_shaders"] }
gpui-component = { version = "=0.5.1" }
```

- `font-kit`: required for text rendering on macOS (Metal path).
- `runtime_shaders`: compiles Metal shaders at app launch instead of requiring
  `xcrun metal` at build time. This removes the hard dependency on full Xcode
  installation; Command Line Tools are sufficient.
- `default-features = false` on gpui: avoids pulling `wayland`, `x11`, and
  `windows-manifest` features which are irrelevant on macOS and add Linux-only
  transitive deps.

## Rust Toolchain

Tested with:

```
rustc 1.96.0 (ac68faa20 2026-05-25)
```

Minimum: latest stable Rust (gpui tracks latest stable).

## macOS Deployment Target

- Metal rendering requires macOS 10.15+ (Catalina).
- Xcode Command Line Tools must be installed (`xcode-select --install`).
- Full Xcode is NOT required when `runtime_shaders` is enabled.
- Tested on macOS 15.x (Sequoia), aarch64.

## Primitives Verified (Phase 1 Spike)

| Primitive | Status | Notes |
|---|---|---|
| `Application::new()` + `app.run()` | Works | crates.io API; no `gpui_platform` needed |
| `gpui_component::init(cx)` | Works | Must be first call inside `app.run` |
| `Root::new(view, window, cx)` | Works | First-level window child |
| `Root::render_dialog_layer` | Works | Overlay rendering |
| `Root::render_sheet_layer` | Works | Overlay rendering |
| `Root::render_notification_layer` | Works | Overlay rendering |
| `InputState::auto_grow(min, max)` | Works | Multiline auto-growing input |
| `InputEvent::PressEnter { secondary }` | Works | Enter vs Shift+Enter discrimination |
| `overflow_y_scrollbar()` (ScrollableElement) | Works | Via trait import |
| `Tree::new(state, render_item)` | Works | Custom row rendering via `ListItem` |
| `TreeState::set_selected_index` | Works | Programmatic selection |
| `h_resizable` / `v_resizable` | Works | Horizontal and vertical resizable panes |
| `resizable_panel().size(px)` | Works | Sized panels |
| `window.open_dialog(cx, \|...\|)` | Works | Modal dialogs |
| Dialog `.overlay_closable(false).keyboard(false).close_button(false)` | Works | Non-dismissable approval dialog |
| `window.open_sheet(cx, \|...\|)` | Works | Sheet slide-in panels |
| `window.push_notification(...)` | Works | Toast notifications with types |
| `Notification::new().title().message()` | Works | Builder API |
| `FocusHandle` / `track_focus` / `Focusable` | Works | Focus tracking |
| `on_action` / `cx.listener` / `KeyBinding` | Works | Action dispatch and keybindings |
| `actions!` macro | Works | Custom action types |

## Known Limitations

1. **`block` crate future-incompat**: transitive dep `block v0.1.6` triggers a
   future-incompatibility warning for Rust editions. Not blocking; tracked
   upstream in Zed.
2. **No `FocusableView` trait**: gpui 0.2.2 uses `Focusable` instead. Docs and
   examples that reference `FocusableView` are outdated.
3. **`TreeEntry` vs `TreeItem`**: the Tree render callback receives `&TreeEntry`
   which wraps a `TreeItem` via `.item()`. Direct field access requires going
   through the accessor.
4. **`Button::primary()`**: requires importing `ButtonVariants` trait.
5. **Dead code warnings**: the existing transport module produces dead-code
   warnings because main.rs no longer calls into it. Acceptable for the spike.

## Upgrade Procedure

1. Check gpui and gpui-component release notes for breaking changes.
2. Update the `=X.Y.Z` version pins in `packages/gui/Cargo.toml`.
3. Run `cargo update -p gpui -p gpui-component` to refresh the lockfile.
4. Run `cargo check -p piko-gui` — fix any API breaks.
5. Run the full spike or a smoke test to verify rendering.
6. Update this document with the new versions and any API migration notes.
7. Coordinate with the integration owner before merging lockfile changes.

## Upstream References

- gpui crate: <https://crates.io/crates/gpui>
- gpui-component crate: <https://crates.io/crates/gpui-component>
- gpui-component docs: <https://longbridge.github.io/gpui-component/docs/>
- Zed repo (gpui source): <https://github.com/zed-industries/zed>
- gpui-component repo: <https://github.com/longbridge/gpui-component>
