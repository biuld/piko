# F-45: Timeline conversation blocks

> Status: implemented
> Priority: P0
> Source evidence: piko product direction after F-44 canvas (user bubble fixed width; thinking/tool not real blocks)
> Design: [D-62](../design/D-62-timeline-conversation-blocks.md)
> Amends: F-44 Timeline row presentation only
> Superseded in part by: F-46 assistant-turn-chips (thinking/tool are chips,
> not collapsible cards)

## Summary

Timeline messages are conversation blocks on a shared island primitive. Hug-to-content (max 72% of the reading column) is the ConversationBlock default; thinking/tool cards opt into `.fill()`. User bubbles wrap and collapse past a 105 px body cap. Assistant is a left-aligned hug chip with a bot icon, selection, and Quote. Thinking and tools are independent collapsible cards with markdown/structured bodies and live streaming carets. Every block supports Copy.

## Problem

User chips used a fixed 72% width. Thinking was a hairline paragraph. Tools dumped truncated JSON. Nothing was selectable or quotable.

## User journeys

1. Short user “hello” hugs the text, trailing, not a wide bar.
2. A long user paste clips to ~5 line-heights with Show more; expand shows all.
3. Assistant markdown hugs like the user chip (left, bot icon), can be selected, right-click Quote inserts `> …` into the Composer.
4. Thinking starts collapsed; while streaming it expands with a caret unless the user collapsed it.
5. A tool card shows name + status; body is key rows or pretty JSON; running is live.
6. Right-click any block copies the selection if present, otherwise the whole block.

## In scope

- Island `ConversationBlock` + markdown Hug width + selectable Copy/Quote extras.
- Piko mapping of `TimelineRow` including streaming and structured tool bodies.

## Out of scope

- hostd/orchd/protocol. Composer chrome, pickers, tabs. Syntax highlighting. Persisted expand state.

## Acceptance criteria

- [x] User bubble is max-width 72% of the current column, not a fixed width.
- [x] Long user bodies clip with Show more.
- [x] Thinking/tool are collapsible cards; thinking is markdown; tools are structured.
- [x] Streaming shows a live caret on a visible body.
- [x] Copy and Quote (user/assistant) without a host intent.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Default width | Hug + max 72% of column on ConversationBlock | Short user/assistant chips must not look like a toolbar; cards call `.fill()` |
| User height | 105 px clip + Show more | Long pastes should not dominate |
| Thinking default | Collapsed; auto-open while streaming if untouched | Secondary, not the document |
| Quote | `> line` + blank line into Composer | Desktop-only, no host |

## Open questions

None.
