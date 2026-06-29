# BottomBar

## Overview

BottomBar is the always-visible status row at the bottom of the TUI. It displays
contextual information about the current session in a compact, non-interactive
format. It is purely read-only — no input, no focus.

## Layout

```
model_prefix/model_id thinking_level · ~/project/dir · context 12.2k/200k · $0.42
```

Items are separated by `·` (U+00B7 middle dot). Each item is a single logical
unit of information. No key hints, help text, or interactive prompts appear in
the BottomBar.

## Items

### 1. Model + thinking level

Displayed as `{provider}/{model_id} {thinking_level}`.

- When no model is configured: `—`
- Thinking level is omitted when it is `off`
- Example: `anthropic/claude-sonnet-4 medium`
- Example: `openai/gpt-4o`

### 2. Project directory

The current working directory, abbreviated to fit.

- Home directory (`~`) expansion
- Truncation from the left when too long: `…/very/deep/nested/project`
- If the path is the home directory itself: `~`
- Example: `~/Projects/piko`

### 3. Context usage

Shows the current context window fill: `used / total`.

- `used` — approximate token count of the current context window
- `total` — model's context window size
- Human-readable: `12.2k/200k`, `1.5k/32k`
- When unknown: `—/—`
- Updates as the conversation grows

### 4. Cost

Estimated cumulative cost for the current session.

- Displayed in USD: `$0.42`
- Blank (`—`) when pricing data is unavailable
- Updates as tokens are consumed

## Configuration

Users can control which items appear and their order via TUI settings stored on
the host. Settings live under the `tui.bottomBar` namespace.

### Available settings

| Key                     | Type      | Default                           | Description                              |
|-------------------------|-----------|-----------------------------------|------------------------------------------|
| `tui.bottomBar.items`   | `string[]` | `["model", "cwd", "context", "cost"]` | Which items to show, in display order    |

### Item identifiers

| Identifier  | Item                |
|-------------|---------------------|
| `model`     | Model + thinking    |
| `cwd`       | Project directory   |
| `context`   | Context usage       |
| `cost`      | Session cost        |

### Settings flow

1. TUI reads `tui.bottomBar` settings from hostd at startup
2. Hostd stores TUI settings alongside other settings (same storage backend)
3. TUI merges defaults with user overrides
4. Future: in-app settings panel to toggle items and reorder
