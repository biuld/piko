# Feature: List keyboard

## Overview

Inside a focused island, flat lists and flattened trees need a single **cursor**
model so apps do not reimplement wrap/activate logic. The kit owns the cursor
machine; the app owns row data and domain meaning of activate / expand.

## Behavior

- Maintain a keyboard cursor index over a visible list of length N.
- Support move previous/next (wrap), home/end, activate, and toggle-expand
  intents.
- Report effects (cursor moved, activate index, toggle-expand index) for the
  app to map to product actions.
- Clamp or clear the cursor when the list length changes.
- Tree/list row paint can show a keyboard focus ring independent of selection.

## Recommended key map (app bindings)

| Key | Intent |
|---|---|
| ↑ / ↓ | Previous / next |
| Home / End | First / last |
| Enter / Space | Activate |
| ← / → (trees) | Toggle expand when the row has children |

## App responsibilities

- Hold list-keyboard state on the island (or pass it through).
- Provide visible row count and map effects to product intents.
- Set which row paints the keyboard focus ring from the cursor.
- Bind keys in an island (or shared) key context.

## Non-goals

- Product messages (open item, select node, …).
- Search-field vs list focus policy inside a sidebar (app).
- Full platform a11y role tree (progressive; not required for core cursor).
