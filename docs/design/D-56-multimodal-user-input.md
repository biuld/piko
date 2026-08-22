# D-56: Multimodal user-input pipeline

> Status: accepted
> Implements: [F-40](../features/F-40-multimodal-user-input.md)

## Goal

Connect piko's existing provider-neutral image representation to the product
submission boundary without bypassing hostd authority or durable agent input.

## Constraints and non-goals

- The session journal remains the sole durable authority.
- `hostd` continues to create and project user-visible Turns.
- Provider-specific image wire shapes remain inside llmd adapters.
- Existing text command JSON remains compatible.
- This slice does not introduce an attachment file store.

## Proposed design

`piko-protocol` adds `ChatSubmitMessage` and `QueueSteerMessage`, each carrying
`MessageContent`. The old string commands are adapters into the new host path.
hostd validates user-originated blocks, derives a bounded text projection for
Turn/queue previews, expands prompt templates only inside text blocks, and
passes the structured content in `AgentRunInput`.

The orch runner already submits `MessageContent` to orchd. Follow-up queue and
transcript DTOs already preserve that content, so no orchd persistence schema
change is required. Steering changes only at the host runner port from `&str`
to owned structured content.

The TUI editor upgrades reference payloads from text-only strings to an enum.
Image IO is an application effect. The clipboard effect reads RGBA pixels and
encodes PNG. A bracketed paste that consists only of an absolute path with a
supported image extension produces a file-read effect instead of mutating the
draft; that effect reads and base64-encodes the original file bytes. Both paths
dispatch the same image-insert action. Submission walks the visible draft and
reference spans in order to produce `MessageContent::Blocks`; text-only drafts
continue using `MessageContent::String`.

Terminals such as Ghostty may deliver a Finder drag as a sequence of ordinary
key events. After each character insertion, the reducer checks only the whole
draft's lexical shape (absolute path plus supported extension). A matching
candidate produces the same file-read effect with an expected-draft token. On
success, a reducer action replaces the draft only if it still byte-matches that
token, preventing stale IO completion from overwriting later input.

```text
clipboard RGBA -> TUI PNG/base64 image reference
local image path -> TUI original MIME/base64 image reference
               -> ChatSubmitMessage(MessageContent)
               -> hostd Turn + AgentRunInput(MessageContent)
               -> orchd durable input/transcript
               -> llmd Responses input_image(data:image/png;base64,...)
               -> provider usage -> llmd pricing policy
```

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Structured submit/steer command DTOs |
| `piko-tui` | Clipboard image effect, image references, structured submission |
| `piko-hostd` | Validation, text projection, structured turn/steer forwarding |
| `piko-orchd` | No new model; consumes its existing `MessageContent` path |
| `piko-llmd` | Verification of Responses image encoding and usage-based pricing |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Clipboard read/encoding failures do not mutate the editor.
- Local image file read failures do not mutate the editor. Non-image and
  non-absolute pasted paths retain the existing text-paste behavior.
- Malformed or unsupported user blocks fail at host admission.
- Model modality mismatch fails in llmd before network dispatch.
- Queued image bytes use the same durable input record and cancellation path as
  text, so recovery cannot orphan an attachment reference.

## Verification

- Protocol serialization compatibility tests for old and new commands.
- Editor tests for insertion, ordering, deletion, image-only submission, and
  restoration.
- Host tests capture structured start and steer input.
- llmd tests inspect Responses JSON and image-inclusive cost calculation.

## Alternatives considered

- Replacing the old command fields in place was rejected because it breaks
  existing JSON-lines clients.
- Persisting temporary image paths was rejected because replay would depend on
  mutable external files.
- Computing image tokens locally was rejected because provider tokenization is
  model-specific and the provider already returns authoritative usage.

## Rollout

1. Protocol and host structured-content boundary.
2. TUI image reference and clipboard effect.
3. Provider wire and billing verification.
