# Design: Reusable TextBox Component

This design document outlines the architecture for introducing a reusable `TextBox` component under `packages/tui/src/ui/components/text_box.rs`. This component will unify all text editing behaviors across different UI surfaces.

## Goal

To abstract text buffer manipulation, multi-byte UTF-8 char boundary navigation, editing actions (inserting, deleting, pasting), and masked rendering into a single, cohesive, reusable widget.

Currently, text input and editing logic is scattered across several disparate locations:
1. **Chat Editor**: The main multi-line prompt editor at the bottom (`AppMode::Chat`).
2. **API Key Input**: A masked text field in the login overlay (`AppMode::AuthSelector`).
3. **Tree Node Renaming**: An inline label editor textbox replacing the node label (`AppMode::Tree` with `LabelEditorState`).
4. **Summary Prompts**: Question workflow overlay when checking tree nodes (`AppMode::SummaryPrompt`).
5. **Tool Interactions**: Input prompts generated dynamically by tools requesting user input (`AppMode::ToolInteraction`).
6. **Overlay Filtering / Search Inputs**:
   - **Sessions Panel Filter**: Search input in `/resume` (stores in `AppState.filter_text`).
   - **Models Panel Filter**: Search input in `/models` (stores in `AppState.filter_text`).
   - **Settings Panel Filter**: Search input in `/settings` (stores in `AppState.filter_text`).
   - **Tree Panel Filter**: Search input in `/tree` to filter visible nodes (stores in `AppState.filter_text`).

*(Note: The Command Palette `/` is not a separate text input; it is an auto-complete suggestion provider that runs directly inside the Chat Editor's text buffer.)*

This fragmentation leads to fragile UTF-8 calculations, inconsistent UX controls, and awkward routing loops for pastes. Our goal is to unify these text buffers.

## Architecture

We will create a standalone, reusable `TextBox` component under `ui/components/text_box.rs`.

Unlike a top-down inheritance model, the `TextBox` is owned and composed by higher-level interactive panels. `AppState` routes keyboard inputs and paste buffers to the active overlay/panel, which in turn delegates core buffer manipulations to its owned `TextBox` instance.

```
                  ┌──────────────────────────────┐
                  │           AppState           │
                  └──────┬────────────────┬──────┘
                         │                │
                         │ routes key /   │ routes key /
                         │ paste          │ paste
                         ▼                ▼
             ┌───────────────────┐┌───────────────────┐
             │       Editor      ││   AuthSelector    │
             │   (Chat Prompt)   ││   (ApiKeyInput)   │
             └───────────┬───────┘└───────┬───────────┘
                         │                │
                         │ composes       │ composes
                         ▼                ▼
                  ┌──────────────────────────────┐
                  │           TextBox            │
                  │        (TextBuffer)          │
                  └──────────────────────────────┘
```

### Component Definition (`packages/tui/src/ui/components/text_box.rs`)

```rust
pub struct TextBox {
    text: String,
    cursor: usize,        // Byte offset in UTF-8 string
    mask_char: Option<char>, // Option to mask rendering (e.g. Some('•') for credentials)
    placeholder: String,  // Muted placeholder text shown when the buffer is empty
    multiline: bool,      // Option to disable multi-line features
}
```

### Public API

```rust
impl TextBox {
    pub fn new() -> Self;
    pub fn with_mask(mask: char) -> Self;
    pub fn with_placeholder(placeholder: impl Into<String>) -> Self;
    pub fn with_multiline(multiline: bool) -> Self;

    // Buffer Operations
    pub fn text(&self) -> &str;
    pub fn set_text(&mut self, text: String);
    pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);

    // Edit Operations
    pub fn insert_char(&mut self, ch: char);
    pub fn insert_str(&mut self, s: &str);
    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str);
    pub fn backspace(&mut self) -> bool;
    pub fn delete_forward(&mut self) -> bool;

    // Cursor Navigation (UTF-8 Safe)
    pub fn move_left(&mut self);
    pub fn move_right(&mut self);
    pub fn move_start(&mut self);
    pub fn move_end(&mut self);
    pub fn cursor_position(&self) -> usize;
    pub fn set_cursor(&mut self, cursor: usize);

    // Rendering Helper
    pub fn render_line(&self, theme: &crate::theme::Theme) -> ratatui::text::Line;
}
```

## Refactoring Plan

1. **Implement `TextBox`**: Write the implementation under `packages/tui/src/ui/components/text_box.rs` and export it in `ui/components/mod.rs`.
2. **Refactor `AuthSelector`**: Change `ApiKeyInput`'s string buffer to use `TextBox`.
3. **Refactor `Tree` (Label Editor)**: Migrate the inline string editor state to use `TextBox`.
4. **Refactor `SummaryPrompt` / `InteractiveWorkflow`**: Replace `input_value: String` with `TextBox`.
5. **Refactor Tool Interactions**: Use `TextBox` in `InteractiveWorkflow` for tool prompt text inputs.
6. **Refactor Overlay/Search Filters**: Eventually migrate the shared `AppState.filter_text` string to a `TextBox` to allow navigation (left/right arrows), home/end shortcuts, and clean text editing inside the filter headers.
7. **Harmonize `Editor` (Chat)**:
   * Eventually, the core text buffer of the `Editor` can delegate to `TextBox` (e.g., using `TextBox` inside the `Editor` struct for raw content operations, allowing `Editor` to focus solely on high-level concerns like multi-line layouts, history, references, and auto-completion).
8. **Simplify input routing in `dispatch.rs`**:
   Rather than performing manual routing across all active overlays in `FilterAppend`, `FilterBackspace`, and `InsertPaste`, we will introduce a helper method `AppState::active_text_box(&mut self) -> Option<&mut TextBox>` to fetch the active text input box and dispatch operations dynamically in a single line.

## Paste Placeholders & Reference Blocks

In the main `Editor` (chat input), pasting large blocks of text (or attaching files/images) does not insert the raw content directly into the visible input area. Instead, it places a reference placeholder (e.g., `[Large Paste: #1]` or `[File: src/main.rs]`) in the text buffer, storing the actual content out-of-band in a `references: Vec<ReferenceBlock>` list. Before submitting to the backend, these placeholders are expanded back into the raw text.

In our unified component design, this capability is separated by layers:

1. **TextBox (Low-Level)**:
   Remains a pure text buffer. It treats a placeholder like `[Large Paste: #1]` as a normal string. It does not know what a "Reference Block" is.
   
2. **Editor (High-Level Controller)**:
   Wraps the `TextBox` and manages the `references` list:
   - **Large Paste Interception**: When `InsertPaste` is called, `Editor` checks if the text exceeds the size threshold. If it does, `Editor` generates a placeholder (e.g. `[Large Paste: #1]`), pushes the `ReferenceBlock` to its own list, and inserts the placeholder string into the `TextBox` (via `textbox.insert_str`).
   - **Atomic Deletions**: When a backspace or delete action occurs, `Editor` checks if the cursor in `TextBox` is adjacent to any known placeholder. If so, it removes the entire placeholder from the `TextBox` at once (by calling `textbox.replace_range`) and removes the corresponding block from `references`. Otherwise, it delegates a normal character backspace to `TextBox`.
   - **Expansion**: When retrieving the final prompt text, `Editor` reads the raw string from `TextBox` and performs string replacements using its `references` table.

This layering ensures that the `TextBox` remains simple and fast for standard input panels (like API keys or session renames) which do not require reference blocks, while allowing the `Editor` to overlay this advanced behavior on top of the shared text buffer logic.

## Rendering Masking and Placeholder Implementation

1. **Placeholder Rendering**:
   When `self.text.is_empty()` is true, the `TextBox` will render the `placeholder` text using the muted color style (e.g. `theme.muted`). If the input holds focus, the cursor `█` is rendered at the start of the field (on top of the first character of the placeholder or immediately preceding it).
   
2. **Masking Rendering**:
   When configured for sensitive inputs (like API keys), the `TextBox` will render `mask_char.unwrap().to_string().repeat(char_count)` instead of the raw string, while maintaining the correct visual cursor position. Placeholder text is *not* masked; it remains fully readable.

