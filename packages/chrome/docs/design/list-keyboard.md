# Design: List / tree keyboard controller

> Parent: [roadmap](../roadmap/README.md) · [list-keyboard feature](../features/list-keyboard.md)  
> Implementation: `src/components/list/list_keyboard.rs`

## Architecture

```text
chrome ListKeyboard          app island Entity
  cursor / wrap / effects  ←→  holds ListKeyboard
  ListKeyIntent              maps ListKeyEffect → product intent
  keyboard_focused paint     supplies row data + handlers
```

**Chrome owns the cursor machine.**  
**App owns data and domain meaning of Activate/ToggleExpand.**

## API shape

- State: `ListKeyboard` (`cursor`, `sync_len`, `ensure_cursor`, `apply`).
- Disabled-row path: `apply_enabled` skips non-interactive rows and aligns with
  `ListRowSpec::enabled`.
- Intents: Prev / Next / Home / End / Activate / ToggleExpand.
- Effects: None / CursorMoved / Activate / ToggleExpand.
- First Prev/Next with empty cursor lands on last/first respectively.
- Tree paint: `TreeRowSpec.keyboard_focused` independent of `selected`.

## Status

- **D3 done:** `ListRowSpec` / `render_list` / `list_row_chrome` for flat nav
  rows; Settings nav consumes the primitive.
- **D4 done:** TreeList composite documented + `tree_row_chrome` pure flags.
- **D5 done:** Settings Nav, Agents, Sessions, and Tree consumers hold
  `ListKeyboard`, paint `keyboard_focused`, and map effects.
- **D6 platform-limited:** GPUI 0.2 does not expose list/tree accessibility-role
  primitives. Keyboard parity and focus-visible paint ship now; semantic roles
  remain an upstream-dependent follow-up.

## Non-goals

- Product messages.
- Search-field vs list focus policy.
