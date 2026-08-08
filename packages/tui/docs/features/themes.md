# Themes

> Status: reviewed

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
| `accent`      | Primary accent: selected items, active marks, links — **not** chrome borders |
| `accentAlt`   | Secondary accent (session labels, alternate states)   |
| `success`     | Success states (completed, assistant label)           |
| `error`       | Error states (failed tools, error labels)             |
| `warning`     | Warning states (running tools, warning notifications) |
| `info`        | Info states (system messages, info notifications)     |
| `border`      | Panel / focused frame chrome                          |
| `borderMuted` | Subtle / unfocused frame chrome                       |

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
| AgentPanel           | `accent` for markers when active, `warning` / `text` / `dim`; chrome: `border` / `borderMuted` |
| Editor               | `text`, `borderMuted` (chrome) |
| NotificationRow      | `info`, `warning`, `error` (by notification level)        |
| BottomBar            | `muted` (body text), `dim` (separator dots)               |
| FilterableList / Pane | `accent` (selected row), `dim` (detail); frame: `border` / `borderMuted` |
| Suggestions          | `accent` (selected), `dim` (detail); frame: `border`      |
| ApprovalPanel        | `warning` (prompt text); frame: `border`                  |
| StatusPanel          | `accent` (key labels), `warning` (preview); frame: `border` |
| HelpPanel            | `text`, `dim`; frame: `border`                            |

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

Authoritative definitions live under `packages/tui/resources/themes/`. Missing
tokens on custom themes are filled from the dark defaults.

### `dark`

Optimized for dark terminal backgrounds. Brand accent is `#5f87ff` for
selection and highlights only. Panel chrome uses neutral `border` / `borderMuted`
— never the accent color.

```toml
[theme]
name = "dark"

[vars]
accent_blue = "#5f87ff"
cyan = "#00d7ff"
# … full palette in resources/themes/dark.toml

[colors]
accent = "accent_blue"
accentAlt = "cyan"
border = "gray"
borderMuted = "dark_gray"
# …
```

### `light`

Optimized for light terminal backgrounds. Same token model; accent stays on
selection, borders stay neutral gray.

See `resources/themes/light.toml` for the full assignment table.
