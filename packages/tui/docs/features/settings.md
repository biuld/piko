# Hierarchical Settings Menu

## Overview

The Settings Menu allows the user to configure runtime settings (such as thinking levels, theme preferences, automatic compaction, and active tools) in a clean, hierarchical tree-structured selector panel.

## Layout

The settings panel opens as a centered partial overlay panel over the timeline view. It displays:
- A header showing the current sub-menu level title (e.g. `settings`, `Thinking Level`, `UI Theme`).
- A list of settings categories (as groups, marked with a `>` suffix) and actions.
- The active values highlighted in the list.
- A brief description of each menu option.

## Behavior / Interactions

- **Opening**: The selector is opened by running the `/settings` slash command, or selecting the `Settings` command from the command palette.
- **Filtering**: Typing characters dynamically filters the list of options in the current active menu level by title or description.
- **Navigation**:
  - `Up` / `Down` arrows select the previous/next menu item.
  - `Enter` on a Group/SubMenu item (e.g. `UI Theme >`) pushes that sub-menu onto the stack, displaying its items and clearing the active filter.
  - `Esc` or `q` pops the top menu off the stack, returning to the parent menu level and clearing the filter. If the stack is at the root level, pressing `Esc` or `q` closes the overlay and returns to the chat view.
- **Confirmation**: Pressing `Enter` on an Action item applies the selected configuration setting on the host, closes the settings panel, and returns to the chat view.

## Configuration Tree Structure

The settings tree is statically declared and mapped to host configuration commands:
- **Thinking Level**: Sets the default reasoning/thinking level of the assistant (`off`, `minimal`, `low`, `medium`, `high`, `xhigh`).
- **Thinking Blocks**: Toggles whether to show or hide thinking content in the timeline rendering.
- **Automatic Compaction**: Enables or disables hostd automatic compaction.
- **UI Theme**: Sets the TUI color theme (`dark`, `light`).
- **Transport Preference**: Sets host transport preference (e.g. `stdio`).
- **Disable tools**: Instantly clears active tools to an empty list.

## Non-goals

- Directly editing text values (such as typing numbers or file paths) within the menu; only pre-defined options and toggles are supported.
