# Timeline Text Selection

## Overview

Timeline message content can be selected and copied like text in a native
document. Selection works without entering an editor, changing transcript
content, or switching away from the conversation.

## Layout

- Selected text uses the system-like selection color derived from the active
  light or dark chrome palette.
- Selection follows the rendered text, including wrapped lines, Markdown
  paragraphs, list items, block quotes, tables, and code blocks.
- No permanent toolbar, copy button, card, or extra gutter is added.

## Behavior and interactions

- Dragging with the primary mouse button selects text within one Timeline row.
- Double-clicking selects a word.
- Starting a selection in another row clears the previous selection.
- User prompts, assistant prose, thinking text, system prose, code, and expanded
  tool arguments or results are selectable.
- Role headings, status labels, icons, tool chips, and buttons are not
  selectable content.
- A selection may cross visual blocks inside one message, but it does not cross
  from one Timeline row into another.
- Right-clicking inside a non-empty selection opens the compact context menu
  with Copy. Right-clicking elsewhere does not replace or extend the selection.
- Copy writes plain text to the system clipboard. Command-C performs the same
  action while a Timeline selection owns focus.
- Copying does not clear the selection. Starting a new selection replaces it.
- Selecting text never activates the underlying row or tool control.
- Append-only streaming preserves the stable row selection state. A body
  rewrite that is not an append clears it conservatively.

## Copied text

The clipboard receives the visible semantic text rather than Markdown source:

- emphasis and links copy their displayed labels;
- list and task markers are retained;
- visual block boundaries use newlines and adjacent inline fragments use
  whitespace;
- table cells and rows retain their visible reading order;
- code copies its code text without fence delimiters or a language badge;
- image alternatives copy their displayed alternative text.

Only `text/plain` is written. Transcript storage remains authoritative and is
never modified by selection or copy.

## Configuration

There is no setting. Selection and the active selected range are ephemeral
window state and are not persisted.

## Non-goals

- Editing Timeline content.
- Selection spanning multiple Timeline rows.
- Select All for the complete transcript.
- Automatic scrolling while dragging beyond the viewport in the first release.
- HTML, rich-text, attributed-string, or Markdown-source clipboard formats.
- Copying hidden metadata, link destinations, or unexpanded tool content.
- Mobile-style selection handles or a persistent copy toolbar.
