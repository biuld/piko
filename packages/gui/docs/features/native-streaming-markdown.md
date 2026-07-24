# Native Streaming Markdown

Status: Stable

## Overview

The Timeline presents assistant prose as Markdown while a response is arriving
and after it is committed. Plain text remains readable because it naturally
falls back to ordinary Markdown text. Rendering stays responsive and does not
switch to a visibly different presentation when the response completes.

## Layout

Markdown fills the assistant message body within the Timeline reading column.
Document structure creates hierarchy inside that body without adding another
card, toolbar, or scroll region around ordinary prose.

- Paragraphs use the conversation body type role.
- Headings use modest size and weight steps appropriate for a conversation.
- Ordered and unordered lists preserve nesting, markers, and continuation
  paragraphs.
- Block quotes use a restrained accent edge and indented content.
- Inline code and fenced code blocks use a monospace presentation.
- Tables remain readable within the available message width.
- Thematic breaks separate sections without becoming heavy panel borders.

## Behavior and interactions

- Assistant text is progressively interpreted as Markdown during generation.
- Incomplete or malformed syntax never hides already received text. The
  renderer shows the best available structure and falls back to literal text
  where needed.
- Completing a response settles the same message in place without inserting a
  duplicate row or changing from plain text to a different document layout.
- Strong, emphasis, strikethrough, inline code, links, hard breaks, headings,
  lists, block quotes, fenced code, tables, and thematic breaks are supported.
- Raw HTML is not interpreted as application UI or executable content.
- Images are represented by their text alternative; remote media is not loaded.
- Links are visually distinguishable, but opening external links is outside the
  first release of this feature.
- Incoming Markdown follows the existing Timeline follow behavior: readers at
  the end continue following, while readers inspecting older content are not
  moved.
- No new keyboard shortcut or command is introduced.

The original transcript text remains authoritative. Parsing and partially
completed structure are window-local presentation state and are not persisted.

## Configuration

There is no user-facing setting. Assistant messages use this presentation by
default, and the active light or dark theme determines all colors.

## Non-goals

- A Markdown editor, source/preview toggle, or WYSIWYG interaction.
- Markdown interpretation for user messages, tool output, logs, or system rows.
- Executing raw HTML, scripts, embedded widgets, Mermaid, or math expressions.
- Downloading remote images or other content referenced by a response.
- Transcript text selection; it is specified separately by
  [Timeline Text Selection](timeline-text-selection.md).
- Rich-text export.
- A fully incremental CommonMark parser in the first release.
