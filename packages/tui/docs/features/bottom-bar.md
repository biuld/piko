# BottomBar

## Overview

BottomBar is the always-visible status row at the bottom of the TUI (shell
chrome). It displays compact session information as items separated by `·`.
It is purely read-only — no input, no focus.

## Layout

```
agent · model_id thinking · ~/project · 12.2k/200k · $0.42
```

### Agent chip

Compact projection of the viewed / primary agent instance:

- Name of active agent (or first agent)
- Busy → name + spinner (accent)
- Multi-agent → `name·N`
- Loading → `…`; empty → `—`
- Full tree UI is Browse surface `Agents` (F4 / keymap AgentPanel), not chrome


## Items

### 1. Model + thinking level

Displayed as `{model_id} {thinking_level}`.

- Dynamically tracks the active model state:
  - **Global default**: Shows the default model from system configuration on startup
  - **Session history**: When opening an existing session, restores the specific model bound to that session's timeline
  - **Live switching**: Updates instantly when changing the global model via the Model Selector
- When no model is configured: `—`
- Thinking level is omitted when it is `off`
- Example: `claude-3-7-sonnet medium`
- Example: `gpt-4o`

### 2. Project directory

The current working directory, abbreviated to fit.

- Home directory (`~`) expansion
- Truncation from the left when too long: `…/very/deep/nested/project`
- If the path is the home directory itself: `~`
- Example: `~/Projects/piko`

### 3. Context usage

Shows the current context window fill: `used / total`.

- `used` — approximate prompt-side tokens from the latest projected model
  usage (`input + cache_read` on the last terminal turn or last assistant
  message with usage). This is the best host-authoritative proxy for window
  fill until a dedicated live estimate is pushed.
- `total` — active model's context window from host
  `ModelEvent::ConfigChanged.contextWindow` when present, otherwise the host
  model catalog warmed at bootstrap
- Human-readable: `12.2k/200k`, `1.5k/32k`
- When both unknown: `—/—`; either side may show `—` when only one is known
- Updates when a session is reconciled, model config changes, or a turn
  completes/fails/cancels with usage

### 4. Cost

Estimated cumulative cost for the current session.

- Source: hostd session `cumulativeUsage` (and live roll-up of terminal turn
  usage between reconciles)
- Displayed in USD: `$0.42` (four decimals for amounts under `$0.01`)
- Blank (`—`) when no usage has been projected yet
- Updates as tokens are consumed

## Configuration

Users can control which items appear and their order via TUI settings stored on
the host. Settings live under the `tui.bottomBar` namespace.

### Available settings

| Key                     | Type      | Default                           | Description                              |
|-------------------------|-----------|-----------------------------------|------------------------------------------|
| `tui.bottomBar.items`   | `string[]` | `["agent", "model", "cwd", "context", "cost"]` | Which items to show, in display order    |

### Item identifiers

| Identifier  | Item                |
|-------------|---------------------|
| `agent`     | Agent chip          |
| `model`     | Model + thinking    |
| `cwd`       | Project directory   |
| `context`   | Context usage       |
| `cost`      | Session cost        |

### Settings flow

1. TUI reads `tui.bottomBar` settings from hostd at startup
2. Hostd stores TUI settings alongside other settings (same storage backend)
3. TUI merges defaults with user overrides
4. Future: in-app settings panel to toggle items and reorder
