# F-44: Conversation canvas presentation

> Status: implemented
> Priority: P0
> Source evidence: piko product direction after F-43 visual review; macOS Tahoe / Liquid Glass HIG
> Design: [D-61](../design/D-61-conversation-canvas-presentation.md)
> Closes: F-43 visual acceptance for the conversation canvas and pickers (Composer shape, Timeline rows/fade, model/thinking chrome). Does **not** reopen tab clustering, sidebar inventory, or follow-tail rules.
> Supersedes: F-43 Composer nested-well anatomy; F-43/D-60 “model/thinking open the existing overlay.” Unlisted F-42 / F-43 rules stand.

## Summary

The desktop conversation canvas is a Tahoe content layer with floating controls. Timeline messages scroll under a floating Composer with a soft scroll-edge fade (not a dark scrim). Rows read as a conversation: trailing user bubble, full-width assistant, secondary thinking, compact tool card, centered system caption. Model and thinking are menus anchored to their toolbar capsules, not modal dialogs. Needs attention and Settings stay dialogs.

## Problem

After F-43 moved agent tabs and model/thinking onto the workspace toolbar, the canvas still painted like a nested tool window: a two-radius Composer well, a flat log of markdown, and catalog pickers as centered dimmed overlays. That fights macOS Tahoe: controls float above content; content is not glass; pickers attach to their source; scroll-edge effects separate floating chrome from scrolling content.

## User journeys

1. The user has a live session with messages. They scroll the Timeline. Older rows pass under the Composer. A soft fade of the content surface (not a dimmer) keeps the last lines readable as they enter that band. Following the tail, the last message sits fully above the Composer card.
2. The Timeline is empty. “No messages yet” sits in the visible column above the Composer. There is no fade because nothing scrolls under the card.
3. The user sends a short note. It appears as a trailing raised bubble. The assistant reply is a full-width document. Thinking is muted and indented. A tool call is a compact inset card, not a second island. A model-change system line is a centered caption.
4. The user opens the model capsule. A menu attaches to the bottom of that capsule (not a 460 px dialog, no dimmer). The current model is checked. Tabs stay usable. Escape or click-outside dismisses without submitting the Composer.
5. The user opens thinking. One list of levels with a checkmark and short captions. Same menu rules.
6. Needs attention still opens a dialog. Settings still opens a dialog.

## In scope

- Soft bottom scroll-edge fade on a ready Timeline; height equals Composer footprint.
- Timeline padding so the last message (and empty-state copy) rest above the Composer.
- Content-layer row styles, reading width, and vertical rhythm.
- Single-radius Composer island (no nested input well).
- Model and thinking as anchored menus; empty catalog still opens a disabled placeholder.
- Keyboard: shell list keys and Composer Enter do not fire through an open menu; TabGroup stays live.

## Out of scope

- Tab clustering, sidebar toggle placement, return-to-latest **rules**.
- In-window Liquid Glass blur (no GPUI within-window backdrop).
- hostd / orchd / protocol / client-core reducer changes.
- Settings information architecture; approval UX beyond keeping the dialog.
- Per-message diffs, tool-output expansion, selectable markdown upgrades.

## Behavior and states

### Composer vs Timeline

- Timeline is the content layer. Composer, `↓ Latest`, and toolbar capsules are the functional layer.
- Ready Timeline: content padding-bottom equals Composer footprint; a **soft** bottom fade of the same height; last row bottom at the fade’s inner (transparent) edge when following.
- Empty / loading / error / no-session: same padding-bottom, **no** fade.
- No top fade while toolbar chrome is a sibling title band.
- Fade does not intercept pointer events.

### Timeline rows

| Kind | Presentation |
|---|---|
| User | Trailing bubble, standard elevated material (not glass), hug content up to ~72% of the reading column |
| Assistant | Leading hug chip (same 72% cap), bot icon, elevated material |
| Thinking | Secondary meta, left hairline, not a bubble |
| Tool | Compact inset content card (hairline, not elevated) |
| System | Centered caption |

Reading column matches Composer max width. First row has no extra leading gap on top of Timeline padding.

### Model and thinking

- **Not** overlays. Anchored menus from the capsules. No dimmer.
- Thinking: one list, checkmark, short inline captions.
- Model: full id in the menu; capsule stays truncated; grouped by provider; checkmark even when catalog context window is 0.
- Empty catalog: one disabled “No models listed” row that still opens.
- Dialog overlays remain for Settings and Needs attention only.

## Acceptance criteria

- [ ] Ready Timeline content can scroll under the Composer with a soft content-color fade, not a dark scrim.
- [ ] Last message at the tail sits fully above the Composer card.
- [ ] Empty/loading/error/no-session copy is not hidden under the Composer.
- [ ] User/assistant/thinking/tool/system rows match the table above (no glass cards).
- [ ] Composer has one outer radius; no nested inner well.
- [ ] Model and thinking open menus attached to their capsules; Escape/outside dismiss; no 45% dimmer.
- [ ] Composer Enter and sidebar list keys do not fire through an open picker menu; TabGroup stays enabled.
- [ ] Needs attention and Settings still open dialogs.
- [ ] Visual acceptance is a user screenshot.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Mask under Composer? | Soft scroll-edge fade, not a scrim | Tahoe scroll-edge; not a modal |
| User vs assistant? | Trailing elevated bubble vs full-width document | Messages-like scan without glass in the content layer |
| Model/thinking overlay? | No; anchored menus | Pickers attach to their control |

## Fusion decisions (codex-rs)

Not derived from codex-rs. Tahoe HIG and F-43 visual review.

## Open questions

None that block this slice. Deferred: true in-window blur; top fade if chrome starts overlaying the scroll viewport; model-catalog search.

## Reference evidence

- Screenshots from 2026-08-22 desktop visual review (nested Composer, dialog pickers, content under the float).
- Apple HIG: Liquid Glass functional layer, scroll-edge effects, menus attach to controls, no glass-on-glass.
- [D-61](../design/D-61-conversation-canvas-presentation.md)
