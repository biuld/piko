# D-64: Two-tone composer with attachments

> Status: design
> Feature: [F-47](../features/F-47-composer-attachments.md)
> Amends: D-59 Slice 4 composer; F-43 control placement

## Visual structure

```
        ┌──────────────┐                    ← peek layer (offset back rect)
┌───────┴──────────────┴────────┐
│  header tone A                │      model ▾   thinking ▾
├───────────────────────────────┤
│  body tone B                  │
│  ┌ auto-growing input ────┐   │
│  └────────────────────────┘   │
│  [chip ×][chip ×]             │      attach chips (bottom row)
│  [+]                 Cancel ⏎ │      action bar
└───────────────────────────────┘
```

- Card: `SurfaceRole::Elevated` fill, island radius, hairline, shadow.
- Header: `SurfaceRole::Content` fill, top corners concentric with card;
  behind it an absolutely-positioned narrower rounded rect (`Sidebar` role,
  lower alpha) offset upward by ~6 px — the peeking second rectangle.
- Header height 32 px; content: leading meta label "Session", then the two
  `ChromeMenuButton`s (model, thinking) reused from the toolbar builders.
- Body keeps `InputBase` + `Textarea` inside a Content-fill inset as today;
  action bar moves under the chip row.

## Attachments

State per agent tab (`AgentViewLocal.attachments: Vec<Attachment>`):

```rust
struct Attachment { id: String, kind: AttachmentKind }
enum AttachmentKind {
    File { path: String },                       // → "@{path}\n" text block
    Image { path: String, data: String, mime_type: String }, // → Image block
}
```

- `+` button: `cx.prompt_for_paths(PathPromptOptions { files: true,
  directories: false, multiple: true })`; on resolve, each path is
  classified by extension (png/jpg/jpeg/gif/webp → image). Images are read +
  base64-encoded immediately; read failure surfaces in `composer_error`.
- Chip rendering: capsule with name (clamped), remove-on-click; reuse of
  ActivityChip is avoided — chips are product composition in composer.rs.
- Submit builds `MessageContent::Blocks`: draft text block first, then one
  text block joining `"@path"` lines for files, then Image blocks. Empty
  attachments keep today's `SubmitTurn { text }` path.

## client-core

New `ClientIntent::SubmitTurnMessage { content: MessageContent }`;
`reduce` resolves the target agent exactly like `SubmitTurn` and emits
`Command::ChatSubmitMessage`. Pending-op bookkeeping reuses the Submit op.

## Island input family

`form/input.rs` gains `TextAreaField`: multiline wrapper around a persistent
`Entity<TextareaState>` (min/max rows, material/surface, click-to-focus),
extracted from the composer's inline styling. The composer consumes it.

## Footprint

Header (32) + peek (6) join `VERTICAL_CHROME`; `footprint_for_text`
constant bumps and its identity test updates (132 → 170).

## Risks

- Prompt dialogs are async app-level; results land via a spawned task that
  upgrades the shell entity — must tolerate a closed window/session switch.
- Chrome menu buttons were styled for the 40 px title band; header reuse
  needs only padding tweaks, no new component.
