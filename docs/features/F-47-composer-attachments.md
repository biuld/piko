# F-47: Composer redesign with attachments

> Status: design
> Priority: P0
> Source evidence: user direction 2026-08-23 (two-tone layered card, offset-stack header, bottom attach chips)
> Design: [D-64](../design/D-64-composer-two-tone.md)
> Amends: F-43 Desktop agent workspace — composer visual + control placement

## Summary

The composer becomes a two-tone layered card. A **header** strip hosts the
session **model** and **thinking-level** pickers (moved out of the window
toolbar); its shape is two offset-stacked rounded rectangles with the upper
one peeking above the card. The body holds the auto-growing input, a bottom
**attachment-chip row**, and the action bar. Attachment chips support
referenced files (`@path`, expanded by hostd mentions) and images
(`ContentBlock::Image`, base64-read client-side per F-40).

## Problem

The flat composer carries no identity: controls that shape the next turn
(model/thinking) live far away in the toolbar, and there is no way to attach
context files or images without typing paths manually.

## User journeys

1. The composer header shows the session model and thinking level; changing
   them no longer requires reaching for the window toolbar.
2. The user clicks `+`, picks a `.png`: an image chip appears at the
   composer's bottom row; submitting sends `ContentBlock::Image`.
3. The user picks a source file: a file chip appears; submitting sends an
   `@/abs/path` mention line the host already expands.
4. Chips are removed individually before sending; drafts persist per tab and
   clear after an accepted submit.

## In scope

- Island `TextAreaField` (multiline gpui-base editor wrapper) completing the
  form family alongside `TextField`.
- Desktop composer: layered two-tone card, offset-stack header, attach-chip
  row (bottom), `+` picker via `prompt_for_paths`, per-tab attachment state.
- client-core `ClientIntent::SubmitTurnMessage { content }` mapped to
  protocol `Command::ChatSubmitMessage`.

## Out of scope

- Drag-and-drop ingestion, clipboard image paste, resizing/transcoding
  (F-40 non-goals), skill mentions UI, queued-steer attachments.

## Acceptance criteria

- [ ] Model/thinking pickers render in the composer header; the window
      toolbar no longer duplicates them.
- [ ] Header paints as two offset stacked rounded rects, upper peeking above
      the card silhouette; card body uses the second tone.
- [ ] `+` opens a multi-select file dialog; image extensions become image
      chips (bytes read immediately, failures surface as composer error),
      other extensions become file-reference chips.
- [ ] Chips render at the composer bottom, removable via click; empty state
      renders nothing.
- [ ] Submit with attachments sends `ChatSubmitMessage` with blocks:
      [draft text] + [`@path` lines] + [Image blocks]; without attachments
      behavior is unchanged.
- [ ] Timeline clearance grows with the new chrome height (footprint test).

## Product decisions

- Pickers live in the composer header (chat-app convention), not the window
  toolbar; tabs and attention stay in the toolbar.
- Files ride the existing `@path` mention expansion instead of new wire
  types; images ride F-40's `ContentBlock::Image`.
