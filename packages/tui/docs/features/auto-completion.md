# Auto-completion

## Overview

Auto-completion is a highly self-contained feature designed to plug into input components (primarily the Editor) to provide real-time suggestions and interactive workflows. It operates by registering specific trigger characters (such as `/` and `@`) and delegating the retrieval, filtering, and selection of items to dedicated sub-features:
1. **Command Palette**: Triggered by `/`, listing available slash commands retrieved from `hostd` for direct execution.
2. **File Browser**: Triggered by `@`, performing global recursive fuzzy search on files in the local workspace.

## Layout

The Auto-completion UI is rendered in Slot D' (directly above the Editor) in the Chat layout.

### Command Palette Layout
```
┌─────────────────────────────────────────────────────┐
│ command palette [1/15] | Tab cycle | Enter execute  │
│ > /help               Show help and shortcuts       │
│   /models             List and set default model    │
│   /sessions           List and open sessions        │
└─────────────────────────────────────────────────────┘
```

### File Browser Layout
```
┌─────────────────────────────────────────────────────┐
│ file browser [2/4] | Tab cycle | Enter accept       │
│   @packages/tui/src/main.rs       file (1.2 KB)     │
│ > @src/main.rs                    file (4.5 KB)     │
│   @src/theme.rs                   file (8.1 KB)     │
└─────────────────────────────────────────────────────┘
```

- **Height**: Dynamically matches the number of completions, up to 8 lines.
- **Borders**: Rendered using the active theme's muted borders.
- **Alignment**: Items are rendered in a two-column left-aligned layout: the left column lists completion labels (commands or paths), and the right column lists details (descriptions or file sizes). The columns align perfectly across all rows.

## Behavior / interactions

### Triggers and Initialization
- **`/` (Command Palette)**: Triggered when the user types `/` as the first character in the Editor or presses `Ctrl-K`.
- **`@` (File Browser)**: Triggered when the user types `@` within the Editor.
- When a trigger is detected, the Auto-completion system activates, fetches initial data, and begins rendering in Slot D'.

### Filtering and Selection (Fuzzy Search)
- **Command Palette (`/`)**: Filters command names and descriptions. Only the primary (first) alias of each command is shown to prevent duplication.
- **File Browser (`@`)**:
  - When the query is empty (just `@`), it displays the top-level files and directories in the current directory (`cwd`).
  - As soon as characters are typed (e.g. `@src`), it switches to a **global recursive fuzzy search**, scanning the entire project workspace (excluding ignored paths like `.git`, `node_modules`, `target`, `dist`, and `build`) to find matching files.
- The list is scrollable, tracking the selected item statefully.

### Navigation and Cycling
- **Tab**: Cycles highlight selection downwards in the list (wrapping to the top after the last item).
- **Shift-Tab**: Cycles highlight selection upwards in the list.
- **Editor Live Preview**: As the user cycles through items, the text in the Editor is **automatically updated** in real-time to show the currently selected item.

### Acceptance and Submission
- **Enter**: Accepts the selected suggestion.
  - **Commands**: If the suggestion is a command, Enter immediately executes it (submits it to `hostd`).
  - **Files**: If the suggestion is a file, Enter inserts the file into the Editor as a placeholder block and closes the autocomplete view.
- **Esc**: Cancels suggestions, closes the view, and leaves the editor's text unchanged.

### Placeholder Blocks
- Files are inserted into the Editor as **placeholder blocks** (e.g., `[@src/main.rs]`).
- The editor treats this placeholder as a **single cohesive unit**: pressing `Backspace` at the end of the block deletes the entire file path at once, rather than deleting character-by-character.
- When the prompt is submitted to the LLM, the editor automatically expands `[@src/main.rs]` back to `@src/main.rs` so that the backend receives clean plain-text file references.

## Configuration

- None.

## Non-goals

- Does not render as a floating overlay that covers editor text.
- Does not walk the directory tree recursively when the query is empty.
