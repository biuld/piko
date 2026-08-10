# Themes

> Status: reviewed

## Overview

The piko TUI theme system is a **fixed semantic color catalog**: every theme
defines the same set of typed slots (backgrounds, role accents, text ramp,
borders, markdown, syntax, …). Paint changes with the theme; structure does
not.

This matches the model used by Grok’s pager theme
(`Theme` slots in xai-grok-pager-render): all colors come from the theme
struct; render code never hard-codes RGB.

Built-in paint lives under `packages/tui/resources/themes/`. Users and projects
may override any subset of slots via TOML in `~/.piko/themes/` or
`.piko/themes/`.

## Design Principles

1. **Semantic over literal** — Slots describe meaning (`accent_user`,
   `success`, `dim`), not “that blue”. Dark and light themes assign different
   concrete colors to the same slots.
2. **Fixed catalog** — A theme always has
   [`Theme::SLOT_COUNT`](../../src/theme/slots.rs) (= **96**) color slots plus a
   display `name`. Missing custom keys fall back to built-in `dark`.
3. **TOML authoring** — Theme files use TOML, consistent with other piko config.
   `[vars]` holds reusable palette colors; `[colors]` maps slots → values.
4. **No hardcoded colors in UI** — Components read `theme.<slot>` (or
   `theme.get("…")` for dynamic lookup by snake_case slot name). Empty string
   `""` means terminal default (`Color::Reset`).
5. **Accent ≠ chrome** — `accent` is selection / active marks only. Panel frames
   use `border` / `border_muted`.

## Catalog (96 slots)

Grouped the same way as the typed `Theme` struct.

### Surfaces — 8

| Slot | Purpose |
|------|---------|
| `bg_base` | Default viewport / body background |
| `bg_elevated` | Raised surface (input, elevated panels) |
| `bg_sunken` | Recessed surface (code blocks) |
| `bg_highlight` | Soft highlight fill |
| `bg_hover` | Hover row fill |
| `bg_selected` | Selected list/tree row background |
| `bg_terminal` | Explicit terminal background |
| `bg_visual` | Text selection background |

### Role accents — 9

Vertical marks / author labels on transcript blocks.

| Slot | Purpose |
|------|---------|
| `accent_user` | User prompt |
| `accent_assistant` | Assistant message |
| `accent_thinking` | Reasoning / thinking |
| `accent_tool` | Tool call |
| `accent_system` | System notice |
| `accent_error` | Error mark |
| `accent_success` | Success mark |
| `accent_running` | In-progress mark |
| `accent_skill` | Skill / slash skill invocation |

### UI / mode accents — 5

| Slot | Purpose |
|------|---------|
| `accent` | Selection, active marks (not borders) |
| `accent_alt` | Secondary marks (session labels) |
| `accent_plan` | Plan mode |
| `accent_model` | Model name chrome |
| `accent_remember` | Remember / pin mode |

### Text hierarchy — 5

| Slot | Purpose |
|------|---------|
| `text` | Primary body text |
| `text_secondary` | Secondary body |
| `dim` | Tertiary / meta punctuation / separators |
| `muted` | Secondary meta, collapsed content |
| `gray_bright` | Bright gray (tool labels) |

### Status — 4

| Slot | Purpose |
|------|---------|
| `success` | Completed / positive outcomes |
| `error` | Failed / error labels |
| `warning` | Running / warning notifications |
| `info` | Info notifications |

### Borders / chrome — 6

| Slot | Purpose |
|------|---------|
| `border` | Focused frame chrome |
| `border_muted` | Unfocused frame chrome |
| `prompt_border` | Composer border (idle) |
| `prompt_border_active` | Composer border (focused) |
| `selection_border` | Selection box edge |
| `hover_border` | Hover outline |

### Content semantic — 3

| Slot | Purpose |
|------|---------|
| `command` | Shell command text |
| `path` | File paths |
| `running` | Live / running indicator color |

### Scrollbar — 2

| Slot | Purpose |
|------|---------|
| `scrollbar_bg` | Track |
| `scrollbar_fg` | Thumb |

### Diff — 6

| Slot | Purpose |
|------|---------|
| `diff_delete_bg` / `diff_delete_fg` | Removed lines |
| `diff_insert_bg` / `diff_insert_fg` | Added lines |
| `diff_equal_fg` | Context / unchanged |
| `diff_gutter_fg` | Line numbers / gutter |

### Transcript blocks — 11

| Slot | Purpose |
|------|---------|
| `user_message_bg` / `user_message_text` | User prompt card |
| `tool_pending_bg` / `tool_success_bg` / `tool_error_bg` | Tool card fill by status |
| `tool_title` / `tool_output` | Tool header / body |
| `custom_message_bg` / `custom_message_text` / `custom_message_label` | Extension messages |
| `thinking_text` | Thinking block body |

### Markdown — 18

| Slot | Purpose |
|------|---------|
| `md_heading_h1` … `md_heading_h6` | Heading levels |
| `md_code` / `md_code_bg` | Inline / fenced code |
| `md_text` / `md_muted` | Body / muted markdown |
| `md_link` / `md_link_url` | Links |
| `md_quote` / `md_quote_border` | Blockquotes |
| `md_hr` | Horizontal rules |
| `md_list_bullet` | List markers |
| `md_task_checked` / `md_task_unchecked` | Task list markers |

### Syntax — 9

`syntax_comment`, `syntax_keyword`, `syntax_function`, `syntax_variable`,
`syntax_string`, `syntax_number`, `syntax_type`, `syntax_operator`,
`syntax_punctuation`.

(Note: fenced code highlighting currently uses bundled syntect themes; these
slots are reserved for token-level paint when the highlight path is fully
token-driven.)

### Thinking level picker — 6

`thinking_off`, `thinking_minimal`, `thinking_low`, `thinking_medium`,
`thinking_high`, `thinking_xhigh`.

### Misc — 4

| Slot | Purpose |
|------|---------|
| `bash_mode` | Bash / shell mode indicator |
| `paste_bg` / `paste_fg` / `paste_dim` | Paste chip / preview |

## File Format

```toml
[theme]
name = "my-theme"

[vars]
blue = "#7aa2f7"
gray = "#6c6c6c"

[colors]
accent = "blue"
border = "gray"
# any of the 96 slots …
```

| Section | Required | Description |
|---------|----------|-------------|
| `[theme]` | yes | `name` (must not contain `/`) |
| `[vars]` | no | Reusable palette; referenced from `[colors]` |
| `[colors]` | yes | Slot → color |

### Color Values

| TOML type | Example | Meaning |
|-----------|---------|---------|
| `"#rrggbb"` | `"#ff0000"` | Hex RGB |
| string | `"blue"` | `[vars]` reference (chainable) |
| integer | `39` | xterm 256 index |
| `""` | `""` | Terminal default (`Reset`) |

## Token-to-component mapping

| Component | Primary slots |
|-----------|----------------|
| Timeline | role accents, `user_message_*`, `tool_*`, `thinking_text`, markdown slots |
| Agent strip | `accent`, `warning`, `success`, `error`, `dim`, `border` / `border_muted` |
| Editor | `text`, plane background (no body fill), `prompt_border` / `prompt_border_active` |
| Lists / palette | `accent`, `bg_selected`, `dim`, `border` / `border_muted` |
| Bottom bar | `muted`, `dim` |
| Notifications | `info`, `warning`, `error` |
| Diff / diagnostics | `diff_*` |

## File locations

| Priority | Location | Scope |
|----------|----------|-------|
| low | built-in `dark` / `light` | shipped |
| mid | `~/.piko/themes/*.toml` | user |
| high | `.piko/themes/*.toml` | project |

Names come from `[theme].name`, not the filename. Higher priority shadows the same name.

## Configuration

```toml
[tui.theme]
name = "dark"
```

Flow:

1. Read `tui.theme.name` from hostd at startup.
2. If unset, auto-detect terminal background → built-in `dark` or `light`.
3. Invalid / missing theme falls back to `dark` with a notification.
4. Custom theme files may hot-reload on save.

## Built-in themes

### `dark`

Neutral gray base (GrokNight-style ramp) with selection accent `#5f87ff`.
Role accents use magenta assistant / thinking, gray tools, blue system.
Full assignment: `resources/themes/dark.toml`.

### `light`

Light gray base (GrokDay-style ramp) with deepened accents for contrast.
Full assignment: `resources/themes/light.toml`.

## Extending the catalog

Adding a slot requires:

1. Field on `Theme` in `src/theme/slots.rs` and entry in the `theme_slots!` list.
2. Assignment in both built-in TOML files.
3. Doc row in this PRD.
4. `Theme::SLOT_COUNT` / tests update.

Do not introduce one-off RGB in feature code.
