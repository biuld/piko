# V-56: Multimodal user-input verification

> Status: implemented
> Verifies: [F-40](../features/F-40-multimodal-user-input.md),
> [D-56](../design/D-56-multimodal-user-input.md)

## Evidence

- Protocol tests preserve legacy text commands and serialize structured
  `MessageContent` commands.
- TUI editor tests preserve mixed text/image ordering, allow image-only
  submission, and restore multimodal queued drafts.
- TUI dispatch/effect tests recognize a pasted absolute image path without
  activating slash suggestions and preserve its MIME type and bytes in the
  inserted image action.
- A Finder-style image path delivered as individual key events is covered by
  a compare-and-replace regression test, including stale-read protection.
- Host turn-runner tests observe image blocks in start and steer requests.
- llmd Responses tests assert `input_image` data URL encoding and capability
  rejection for text-only targets.
- DeepSeek catalog/billing tests apply time-of-day pricing to provider-reported
  input usage for `deepseek-v4-flash-vision-exp`.

## Commands

```bash
cargo test -p piko-protocol
cargo test -p piko-tui
cargo test -p piko-hostd
cargo test -p piko-llmd
cargo clippy --workspace --all-targets -- -D warnings
```
