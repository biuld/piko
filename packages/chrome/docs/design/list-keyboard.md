# Design: List / tree keyboard controller

> Parent: [roadmap](../roadmap/README.md) · [list-keyboard feature](../features/list-keyboard.md)  
> Implementation: `src/chrome/list/list_keyboard.rs`

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
- Intents: Prev / Next / Home / End / Activate / ToggleExpand.
- Effects: None / CursorMoved / Activate / ToggleExpand.
- First Prev/Next with empty cursor lands on last/first respectively.
- Tree paint: `TreeRowSpec.keyboard_focused` independent of `selected`.

## Status

- **D3 done:** `ListRowSpec` / `render_list` / `list_row_chrome` for flat nav
  rows; Settings nav consumes the primitive.
- **D4 done:** TreeList composite documented + `tree_row_chrome` pure flags;
  Agents island is the primary full path (`ListKeyboard` + `keyboard_focused` +
  `render_tree_list`). Display-only trees may paint via chrome with
  `keyboard_focused: false` without inventing a second row chrome.
- **D5 done:** primary consumers hold `ListKeyboard` and map effects.
- **D6 todo:** progressive a11y roles.

## Non-goals

- Product messages.
- Search-field vs list focus policy.
