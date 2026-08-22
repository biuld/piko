# F-40: Multimodal user input

> Status: reviewed
> Priority: P1
> Source evidence: [DeepSeek image-understanding contract](https://api-docs.deepseek.com/zh-cn/guides/vision/)

## Summary

Users can submit text and image content as one message to an image-capable
model. The same structured content survives immediate starts, durable
follow-ups, steering, transcript persistence, replay, and provider encoding.

## Problem

piko's transcript and model gateway understand image blocks, but the client and
host submission boundary accepts only text. Advertising an image-capable model
without a complete input path makes the capability unusable and misleading.

## User journeys

1. The user selects an image-capable model, pastes an image into the composer,
   adds optional text, and submits it. The timeline shows an image placeholder
   and the provider receives one structured multimodal message.
2. The user drags a local image file into the terminal. The terminal's pasted
   absolute path becomes the same atomic image attachment placeholder instead
   of editor text or a slash-command query.
3. The user queues a multimodal follow-up while an agent is busy. The exact
   content starts later in FIFO order and remains durable across recovery.
4. The user submits an image to a text-only target. The model gateway rejects
   the request as an unsupported capability before provider dispatch.

## In scope

- Structured text/image submit and steer commands.
- Clipboard image ingestion and pasted absolute local-image paths in the TUI,
  with a visible atomic placeholder.
- Base64 image blocks with an explicit MIME type.
- Preservation through host turn registration, orchd admission, durable
  follow-up queues, transcript commits, and Responses encoding.
- Cost accounting from provider-reported input tokens, including image tokens
  converted by the provider.
- Compatibility for existing text-only commands and clients.

## Out of scope

- Remote image URLs.
- Image resizing, transcoding policy, or OCR.
- A piko-local estimate of provider image billing tokens.
- Rendering bitmap pixels inside the terminal timeline.

## Behavior and states

- A composed image is represented by an atomic placeholder; deleting the
  placeholder removes the image.
- Submitting resolves placeholders into ordered `ContentBlock` values. Text
  surrounding an image stays in the same relative order.
- An image-only message is valid. Empty text without an image remains a no-op.
- Clipboard failures leave the draft unchanged and produce a visible error.
- A bracketed paste containing only an absolute path with a supported image
  extension (`png`, `jpg`/`jpeg`, `gif`, or `webp`) reads that file as an image
  attachment. Read failures leave the draft unchanged and produce a visible
  error; other pasted paths remain ordinary text.
- Some terminals deliver a Finder/file-manager drag as ordinary key events
  rather than bracketed paste. When the complete composer draft resolves to a
  supported absolute image path, a successful read atomically replaces that
  path with the image placeholder. The path remains unchanged on failure.
- Text-only slash commands remain text-only and cannot contain attachments.
- Dequeuing a local multimodal follow-up restores both text and images.
- hostd validates that user content is non-empty and contains only text/image
  blocks before starting or steering a turn.
- Provider-reported `input_tokens` remains the billing authority. piko applies
  the selected model's pricing policy to those tokens.

## Acceptance criteria

- [x] A TUI clipboard image becomes a structured user image block rather than
      placeholder text in the model request.
- [x] A pasted absolute local-image path becomes an atomic image attachment
      and does not activate slash-command suggestions.
- [x] Text and image block order is preserved through hostd and orchd.
- [x] Immediate, follow-up, and steer delivery accept structured content.
- [x] Existing `ChatSubmit` and `QueueSteer` text commands remain valid.
- [x] DeepSeek Responses emits `input_image` with a base64 data URL.
- [x] Provider-returned image-inclusive input usage is priced by the configured
      DeepSeek time-of-day policy.
- [x] Unsupported image targets fail before dispatch.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where are image bytes durable? | In the existing transcript `ContentBlock::Image` | One authoritative replay path; no attachment side store or broken references |
| What is the billing authority? | Provider-reported usage | Image tokenization is provider/model specific |
| How is compatibility retained? | Add structured command variants and adapt old text variants | Existing JSON-lines clients continue to work |
| Which clipboard representation is sent? | PNG encoded from clipboard RGBA pixels | Deterministic MIME type accepted by supported vision providers |

## Open questions

None for this slice.

## Reference evidence

- DeepSeek Vision guide: <https://api-docs.deepseek.com/zh-cn/guides/vision/>
- Existing provider-neutral `ContentBlock::Image` and Responses
  `input_image` encoder.
