# Interactive Login Overlay

## Overview

The Interactive Login Overlay provides a user-friendly, keyboard-navigable interface for users to authenticate with LLM providers using either an API Key or a web subscription (OAuth). It replaces the previous behavior where typing `/login` immediately failed or launched an unsupported flow.

## Layout

The selector opens as a centered partial overlay panel over the timeline view (using `Placement::Partial` slot allocation). It has two main states:

1. **Selection Menu State**:
   - A list view presenting top-level choices: `Use a subscription (OAuth)` and `Use an API key`.
   - Sub-menus showing providers supporting each respective auth method.
   - Arrow indicators `>` indicating sub-menus.

2. **API Key Input State**:
   - Prompts the user: `Enter API key for <provider>:`
   - A masked text input field displaying `*` for each typed character to prevent shoulder-surfing.
   - Help text showing: `Enter to submit · Esc to go back`.

## Behavior / Interactions

- **Opening**: The overlay is opened by running the `/login` slash command (without arguments) or selecting `Login` from the command palette.
- **Menu Navigation**:
  - `Up` / `Down` arrows move the selection.
  - `Enter` confirms the selection, entering sub-menus or confirming the action.
  - `Esc` goes back one level in the hierarchy, or closes the overlay if at the root menu.
- **API Key Entry**:
  - Typing adds characters to the API key.
  - `Backspace` deletes characters.
  - `Enter` submits the key to `hostd` (triggers config updates) and closes the overlay.
  - `Esc` cancels key entry and returns to the provider selection menu.
- **Direct Login Bypass**: Running `/login <provider>` (e.g., `/login openai`) immediately triggers the OAuth flow for that provider in the background, bypassing the overlay entirely.

## Non-goals

- Implementing new OAuth providers in the backend.
- Managing OAuth token refresh rates in the TUI (handled entirely by `hostd`).
- Supporting API key verification inside the TUI prior to sending the command to `hostd`.
