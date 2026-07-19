# GPUI Dependency Pins

> Phase 1 spike result — compatibility verified on macOS (aarch64). Pins remain
> the live dependency policy for `piko-gui`.

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
| `window.open_sheet(cx, \|...\|)` | Works | Sheet slide-in panels (dock fallback) |
| `window.push_notification(...)` | Works | Toast notifications with types |
| `Notification::new().title().message()` | Works | Builder API |
| chrome `OverlayHost` (custom absolute layer) | Works | Product center modals; not `open_dialog` |
| `FocusHandle` / `track_focus` / `Focusable` | Works | Focus tracking |
| `on_action` / `cx.listener` / `KeyBinding` | Works | Action dispatch and keybindings |
| `actions!` macro | Works | Custom action types |

## API notes

1. **`block` crate future-incompat**: transitive dep `block v0.1.6` may trigger
   a future-incompatibility warning. Not blocking; tracked upstream in Zed.
2. **No `FocusableView` trait**: gpui 0.2.2 uses `Focusable`. Docs/examples that
   reference `FocusableView` are outdated.
3. **`TreeEntry` vs `TreeItem`**: the Tree render callback receives `&TreeEntry`
   which wraps a `TreeItem` via `.item()`.
4. **`Button::primary()`**: requires importing `ButtonVariants`.

## Upgrade Procedure

1. Check gpui and gpui-component release notes for breaking changes.
2. Update the `=X.Y.Z` version pins in `packages/gui/Cargo.toml`.
3. Run `cargo update -p gpui -p gpui-component` to refresh the lockfile.
4. Run `cargo check -p piko-gui` — fix any API breaks.
5. Run focused GUI tests and a real-hostd smoke when practical.
6. Update this document with the new versions and any API migration notes.

## hostd discovery

`piko-gui` resolves the hostd binary in this order (see
`src/transport/discovery.rs`):

1. `PIKO_HOSTD_PATH` or `PIKO_HOSTD_COMMAND`
2. `piko-hostd` adjacent to the running `piko-gui` executable
3. `<workspace-root>/target/debug/piko-hostd`
4. `<workspace-root>/target/release/piko-hostd`
5. bare `"piko-hostd"` via `PATH`

## Upstream References

- gpui crate: <https://crates.io/crates/gpui>
- gpui-component crate: <https://crates.io/crates/gpui-component>
- gpui-component docs: <https://longbridge.github.io/gpui-component/docs/>
- Zed repo (gpui source): <https://github.com/zed-industries/zed>
- gpui-component repo: <https://github.com/longbridge/gpui-component>
