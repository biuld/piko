# Feature: Context Menu

## Overview

Context menus provide a compact, temporary action surface anchored at the
pointer. They are intended for secondary actions on list and tree rows or
selected document content without changing the active workspace or opening a
modal overlay.

## Layout

- The menu appears beside the secondary-click position and remains fully
  inside the window with an 8 px safe margin.
- Its width follows its labels within compact minimum and maximum bounds.
- The surface uses tonal elevation: the active palette's `elevated` color, a
  restrained border, and an 8 px radius, without a drop shadow.
- Items use the standard compact row height and typography. Destructive items
  use danger-colored text without a permanently filled danger background.

## Behavior

- Right-click opens the menu. On macOS, Control-click has the same behavior.
- A consumer may return no available items for a pointer position; in that case
  no empty menu surface opens.
- Opening a menu does not activate the underlying row.
- Pointer hover changes the active item; clicking an enabled item invokes it
  once and closes the menu.
- A pointer-opened menu starts without an active item. The first Down selects
  the first enabled item; the first Up selects the last. Enter is a no-op until
  an item is active.
- Up and Down move through enabled items with wrapping. Enter invokes the
  active item. Escape closes the menu.
- Disabled items and separators are skipped by keyboard navigation and cannot
  be invoked.
- Clicking outside closes the menu without invoking an action.
- Closing restores the focus that existed before the menu opened.
- A menu action may subsequently open a modal overlay; the context menu closes
  before that transition begins.

## State and configuration

Menu open state, keyboard selection, pointer position, and focus restoration
are ephemeral window presentation state. They are not persisted and require no
configuration or host communication.

The consuming app supplies localized labels, enabled state, destructive
semantics, and callbacks that map selection to product actions.

## Non-goals

- Nested submenus.
- Checkbox, radio, shortcut-hint, or arbitrary custom-element rows.
- Scrolling menus or command-palette-sized collections.
- Application menu-bar replacement.
- Modal priority, dimmed backdrops, or participation in the product overlay
  stack.
- Product commands, confirmation policy, or localization catalogs.
