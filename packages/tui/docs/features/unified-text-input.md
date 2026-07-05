# Unified Text Input Component

## Overview

The Unified Text Input component (internally named `TextBox`) is a reusable UI component that standardizes text manipulation, key navigation, character deletions, and credential masking across all interactive surfaces of the terminal application.

## Layout

The text input appears across multiple contexts in the application:
- **Chat Editor**: The main prompt editor at the bottom of the screen (which also serves as the input field for the Command Palette `/` when listing slash commands).
- **API Key Configuration**: A masked text field inside the OAuth/Login configuration panel.
- **Tree Node Renaming**: An inline input box that replaces a node's label when modifying its name in the session tree.
- **Summary Prompts**: Custom text fields within interactive workflows, such as providing custom instructions when resuming branches.
- **Tool Interactions**: Input fields dynamically prompted by tools that require user answers or confirmation during task execution.
- **Global search/filter inputs**: The header filter text bar found in the Sessions panel (`/resume`), Models panel (`/models`), Settings panel (`/settings`), and Tree view (`/tree`).

Each text input displays:
- A text area showing either the typed characters, masked characters (e.g. `•`), or a grayed-out placeholder text (e.g. `Enter API key...`) when the input is empty.
- A block cursor `█` indicating the active insertion point.
- Support for inline horizontal scrolling when input exceeds the display area width (if single-line).

## Behavior / Interactions

Regardless of where it is used, the unified text input guarantees consistent keyboard shortcuts and edit behaviors:

- **Character Input**: Typing standard keys appends characters at the cursor position.
- **Pasting**: Pasting multiline or single-line text (via bracketed paste) inserts text at the cursor position.
- **Deletion**:
  - `Backspace` deletes the character preceding the cursor.
  - `Delete` (Forward Delete) deletes the character following the cursor.
- **Navigation**:
  - `Left` / `Right` arrows move the cursor one character to the left/right, respecting multibyte character boundaries.
  - `Home` / `Ctrl+A` jumps the cursor to the beginning of the current input line.
  - `End` / `Ctrl+E` jumps the cursor to the end of the current input line.
- **Masking**: When configured for sensitive inputs (like API keys), characters are rendered as bullet points, but selection, cursor navigation, deletions, and pasting behave identically to normal text fields.

## Configuration

The component supports configurations for:
- `multiline`: Toggle between single-line input (Enter submits, tabs navigate) and multiline code editor behaviors.
- `mask_char`: Set a character to visually override and hide raw sensitive text.
- `placeholder`: Define hint text to display when the input buffer is empty.

## Non-goals

- Implementing rich text formatting or syntax highlighting directly inside the base `TextBox` (delegated to high-level wrappers or themes).
- Supporting complex visual selection block highlighting or terminal-based mouse selection of sub-strings inside the single-line input.
