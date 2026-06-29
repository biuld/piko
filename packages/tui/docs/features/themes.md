# Themes

## Overview

The piko theme system controls all colors in the TUI through a set of semantic
color tokens. Theme files are TOML documents that assign concrete colors to
each token. Switching themes changes the entire visual appearance without
modifying any rendering code.

## Design Principles

1. **Semantic over literal** — Tokens describe meaning (`success`, `error`,
   `dim`), not appearance (`green`, `red`, `gray`). A "dark" theme and a
   "light" theme define different concrete colors for the same semantic token.

2. **TOML format** — Theme files use TOML, consistent with piko's other
   configuration files. TOML sections naturally separate metadata, variables,
   and color definitions.

3. **No hardcoded colors** — Every component renders with theme tokens. The
   only exception is "default terminal color" (token `text` resolving to `""`).

4. **Extensible** — Built-in themes ship with piko. Users and projects can add
   custom themes in `~/.piko/themes/` and `.piko/themes/`. Priority:
   built-in < global < project.

## File Format

Theme files use TOML with three sections:

```toml
# piko theme: my-theme
# Custom theme description.

[theme]
name = "my-theme"

[vars]
blue = "#5f87ff"
gray = 242

[colors]
accent = "blue"
border = "blue"
borderAccent = "#00d7ff"
borderMuted = "gray"
success = "#b5bd68"
error = "#cc6666"
warning = "#ffff00"
muted = "#808080"
dim = "#666666"
text = ""
```

| Section   | Required | Description                                              |
|-----------|----------|----------------------------------------------------------|
| `[theme]` | yes      | Metadata: `name` (unique, must not contain `/`)          |
| `[vars]`  | no       | Reusable color variables. Keys are referenced by name in `[colors]` |
| `[colors]` | yes     | All color token assignments (see Token Reference)         |

### Color Values

A color value in `[vars]` or `[colors]` can be one of:

| TOML type      | Example      | Meaning                                |
|---------------|-------------|----------------------------------------|
| string `"#…"`  | `"#ff0000"` | 6-digit hex RGB                        |
| string (other) | `"blue"`    | Variable reference to a key in `[vars]` |
| integer        | `39`         | xterm 256-color palette index (0–255)   |
| string `""`    | `""`        | Terminal's default foreground/background |

A string that matches a `[vars]` key is resolved through that variable — so
`"blue"` in `[colors]` looks up the `blue` key in `[vars]`. Hex values are
distinguished by the `#` prefix. Variable references can chain: `a = "b"`,
`b = "#00ff00"` is valid. Circular references are detected and rejected.

## Token Reference

### Layer 1 — Core UI

These tokens are actively used by all components.

| Token          | Purpose                                              |
|---------------|------------------------------------------------------|
| `text`        | Default body text                                    |
| `dim`         | Tertiary / very dim text (details, placeholders)      |
| `muted`       | Secondary / muted text (descriptions, metadata)       |
| `accent`      | Primary accent (selected items, active states)        |
| `accentAlt`   | Secondary accent (session labels, alternate states)   |
| `success`     | Success states (completed, assistant label)           |
| `error`       | Error states (failed tools, error labels)             |
| `warning`     | Warning states (running tools, warning notifications) |
| `info`        | Info states (system messages, info notifications)     |
| `border`      | Normal panel borders                                  |
| `borderAccent` | Highlighted / focused panel borders                 |
| `borderMuted`  | Subtle borders (agent panel top line)                |

### Layer 2 — Extended

These tokens are parsed and reserved for planned features (markdown rendering,
syntax highlighting, tool diffs).

| Token group   | Count | Tokens                                                                 |
|---------------|-------|------------------------------------------------------------------------|
| Markdown      | 10    | `mdHeading`, `mdLink`, `mdLinkUrl`, `mdCode`, `mdCodeBlock`, `mdCodeBlockBorder`, `mdQuote`, `mdQuoteBorder`, `mdHr`, `mdListBullet` |
| Syntax        | 9     | `syntaxComment`, `syntaxKeyword`, `syntaxFunction`, `syntaxVariable`, `syntaxString`, `syntaxNumber`, `syntaxType`, `syntaxOperator`, `syntaxPunctuation` |
| Tool diffs    | 3     | `toolDiffAdded`, `toolDiffRemoved`, `toolDiffContext`                  |
| Thinking      | 6     | `thinkingOff`, `thinkingMinimal`, `thinkingLow`, `thinkingMedium`, `thinkingHigh`, `thinkingXhigh` |
| Other         | 3     | `thinkingText`, `bashMode`, `toolOutput`                               |

### Layer 3 — Backgrounds

Optional background tokens. When unset, the terminal default background is used.

| Token              | Purpose                           |
|--------------------|-----------------------------------|
| `selectedBg`       | Selected list item background      |
| `userMessageBg`    | User message card background       |
| `customMessageBg`  | Extension message background       |
| `toolPendingBg`    | Tool box (pending)                 |
| `toolSuccessBg`    | Tool box (success)                 |
| `toolErrorBg`      | Tool box (error)                   |
| `userMessageText`  | User message text color            |
| `customMessageText` | Extension message text color      |
| `customMessageLabel` | Extension message label color    |
| `toolTitle`        | Tool box title color               |

### Token-to-Component Mapping

Where each token is used:

| Component            | Tokens used                                              |
|----------------------|----------------------------------------------------------|
| Timeline             | `text`, `dim`, `accent` (system), `accentAlt` (session), `success` (assistant), `error`, `warning` (tool running), `border`, `userMessageBg`, `toolPendingBg`, `toolSuccessBg`, `toolErrorBg` |
| AgentPanel           | `accent` (idle marker), `warning` (active marker), `text` (agent name), `dim` (queue count), `borderMuted` (top line) |
| Editor               | `text`, `borderMuted` (normal mode), `borderAccent` (command/approval active) |
| NotificationRow      | `info`, `warning`, `error` (by notification level)        |
| BottomBar            | `muted` (body text), `dim` (separator dots)               |
| FilterableList       | `accent` (selected item), `dim` (detail), `borderMuted`   |
| Suggestions          | `accent` (selected), `dim` (detail), `borderMuted`        |
| ApprovalPanel        | `warning` (prompt text), `warning` (border)               |
| StatusPanel          | `accent` (key labels), `warning` (preview text), `borderMuted` |
| HelpPanel            | `text`, `dim`, `borderMuted`                              |

## File Locations

Themes are discovered from the following locations, in priority order (higher
priority overrides lower):

| Priority | Location             | Scope   | Example path                  |
|----------|---------------------|---------|-------------------------------|
| 1 (low)  | Built-in            | shipped | `dark`, `light`               |
| 2        | Global user themes  | user    | `~/.piko/themes/*.toml`       |
| 3 (high) | Project themes      | project | `.piko/themes/*.toml`         |

### Name Resolution

Theme names come from the `name` field inside `[theme]` (not the filename).
Two files with the same `name` cause the higher-priority location to shadow the
lower one. A project-level theme overrides a global theme with the same name.

### Custom Themes

Custom themes follow the same TOML format. Create a file like
`~/.piko/themes/catppuccin.toml`:

```toml
[theme]
name = "catppuccin"

[vars]
rosewater = "#f5e0dc"
mauve = "#cba6f7"
# ...

[colors]
accent = "mauve"
border = "mauve"
# ...
```

The theme then appears in the `/settings` selector alongside built-in themes.

## Configuration

### Selecting a Theme

Users set the active theme in `settings.toml`:

```toml
[tui.theme]
name = "dark"
```

Or through the `/settings` panel in the TUI.

### Settings Flow

1. TUI reads `tui.theme.name` from hostd settings at startup.
2. If unset, piko auto-detects terminal background (dark/light) and picks the
   corresponding built-in theme.
3. The resolved theme is loaded and used by all rendering functions.
4. If the theme file is missing or invalid, piko falls back to built-in `dark`
   and emits a notification.

### Hot Reload

When the active theme is a custom file (not built-in), piko watches the file
for changes. Saving the file triggers an immediate reload. If the file becomes
invalid while being edited, the last valid state is kept and an error
notification is shown.

## Built-in Themes

piko ships with two built-in themes: `dark` (default) and `light`.

### `dark`

Optimized for dark terminal backgrounds.

```toml
[theme]
name = "dark"

[vars]
cyan = "#00d7ff"
blue = "#5f87ff"
green = "#b5bd68"
red = "#cc6666"
yellow = "#ffff00"
text_color = "#d4d4d4"
gray = "#808080"
dim_gray = "#666666"
dark_gray = "#505050"
accent_color = "#8abeb7"
selected_bg = "#3a3a4a"
user_msg_bg = "#343541"
tool_pending_bg = "#282832"
tool_success_bg = "#283228"
tool_error_bg = "#3c2828"
custom_msg_bg = "#2d2838"

[colors]
accent = "accent_color"
accentAlt = "blue"
border = "blue"
borderAccent = "cyan"
borderMuted = "dark_gray"
success = "green"
error = "red"
warning = "yellow"
info = "blue"
muted = "gray"
dim = "dim_gray"
text = "text_color"

thinkingText = "gray"
selectedBg = "selected_bg"
userMessageBg = "user_msg_bg"
userMessageText = "text_color"
customMessageBg = "custom_msg_bg"
customMessageText = "text_color"
customMessageLabel = "#9575cd"
toolPendingBg = "tool_pending_bg"
toolSuccessBg = "tool_success_bg"
toolErrorBg = "tool_error_bg"
toolTitle = "text_color"
toolOutput = "gray"
mdHeading = "#f0c674"
mdLink = "#81a2be"
mdLinkUrl = "dim_gray"
mdCode = "accent_color"
mdCodeBlock = "green"
mdCodeBlockBorder = "gray"
mdQuote = "gray"
mdQuoteBorder = "gray"
mdHr = "gray"
mdListBullet = "accent_color"
toolDiffAdded = "green"
toolDiffRemoved = "red"
toolDiffContext = "gray"
syntaxComment = "#6A9955"
syntaxKeyword = "#569CD6"
syntaxFunction = "#DCDCAA"
syntaxVariable = "#9CDCFE"
syntaxString = "#CE9178"
syntaxNumber = "#B5CEA8"
syntaxType = "#4EC9B0"
syntaxOperator = "#D4D4D4"
syntaxPunctuation = "#D4D4D4"
thinkingOff = "dark_gray"
thinkingMinimal = "#6e6e6e"
thinkingLow = "#5f87af"
thinkingMedium = "#81a2be"
thinkingHigh = "#b294bb"
thinkingXhigh = "#d183e8"
bashMode = "green"
```

### `light`

Optimized for light terminal backgrounds.

```toml
[theme]
name = "light"

[vars]
teal = "#5a8080"
blue = "#547da7"
green = "#588458"
red = "#aa5555"
yellow = "#9a7326"
text_color = "#1f2328"
medium_gray = "#6c6c6c"
dim_gray = "#767676"
light_gray = "#b0b0b0"
selected_bg = "#d0d0e0"
user_msg_bg = "#e8e8e8"
tool_pending_bg = "#e8e8f0"
tool_success_bg = "#e8f0e8"
tool_error_bg = "#f0e8e8"
custom_msg_bg = "#ede7f6"

[colors]
accent = "teal"
accentAlt = "blue"
border = "blue"
borderAccent = "teal"
borderMuted = "light_gray"
success = "green"
error = "red"
warning = "yellow"
info = "blue"
muted = "medium_gray"
dim = "dim_gray"
text = "text_color"

thinkingText = "medium_gray"
selectedBg = "selected_bg"
userMessageBg = "user_msg_bg"
userMessageText = "text_color"
customMessageBg = "custom_msg_bg"
customMessageText = "text_color"
customMessageLabel = "#7e57c2"
toolPendingBg = "tool_pending_bg"
toolSuccessBg = "tool_success_bg"
toolErrorBg = "tool_error_bg"
toolTitle = "text_color"
toolOutput = "medium_gray"
mdHeading = "yellow"
mdLink = "blue"
mdLinkUrl = "dim_gray"
mdCode = "teal"
mdCodeBlock = "green"
mdCodeBlockBorder = "medium_gray"
mdQuote = "medium_gray"
mdQuoteBorder = "medium_gray"
mdHr = "medium_gray"
mdListBullet = "green"
toolDiffAdded = "green"
toolDiffRemoved = "red"
toolDiffContext = "medium_gray"
syntaxComment = "#008000"
syntaxKeyword = "#0000FF"
syntaxFunction = "#795E26"
syntaxVariable = "#001080"
syntaxString = "#A31515"
syntaxNumber = "#098658"
syntaxType = "#267F99"
syntaxOperator = "#000000"
syntaxPunctuation = "#000000"
thinkingOff = "light_gray"
thinkingMinimal = "#767676"
thinkingLow = "blue"
thinkingMedium = "teal"
thinkingHigh = "#875f87"
thinkingXhigh = "#8b008b"
bashMode = "green"
```
