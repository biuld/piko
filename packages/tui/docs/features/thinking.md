# Thinking Level Selector

## Overview

The Thinking Level Selector allows the user to select and configure the reasoning/thinking level of the active assistant model. The selector displays all supported thinking levels in a list view.

## Layout

The selector opens as a centered partial overlay panel over the timeline view (it is rendered using the settings panel view). It displays:
- A filter input header showing the current sub-menu title `Thinking Level`.
- A list of all available thinking levels.
- The currently active thinking level highlighted in the list.
- A brief description of each level next to its label.

## Behavior / Interactions

- **Opening**: The selector is opened by running the `/thinking` slash command, or selecting the `Thinking level` command from the command palette.
- **Filtering**: Typing character keys dynamically filters the thinking levels list by their label or description.
- **Navigation**:
  - `Up` / `Down` arrows select the previous/next visible option.
  - `Esc` or `q` closes the selector overlay and returns to the chat view.
- **Confirmation**: Pressing `Enter` applies the selected thinking level to the host, closes the selector, and updates the active settings.

## Configuration

The thinking levels supported are:
- `off`: Disable assistant thinking/reasoning entirely.
- `minimal`: Use minimal reasoning budget.
- `low`: Use low reasoning budget.
- `medium`: Use medium reasoning budget.
- `high`: Use high reasoning budget.
- `xhigh`: Use extra high reasoning budget (maximum).

## Non-goals

- Restricting selection based on whether the active model natively supports reasoning; the user can select any level, and the host will map or ignore it as appropriate.
- Persisting session-specific thinking overrides beyond standard host configuration changes.
