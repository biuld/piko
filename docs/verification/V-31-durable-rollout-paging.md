# V-31: F-15 durable rollout paging

> Feature: [F-15](../features/F-15-observability.md)
> Design: [D-31](../design/D-31-durable-rollout-paging.md)
> Date: 2026-08-06

## Evidence

| Criterion | Evidence |
|---|---|
| Opaque cursor validation | `rollout_cursor_is_opaque_and_round_trips` |
| Stable command JSON | `rollout_page_get_round_trips` |
| Ordered, non-overlapping durable pages | `rollout_pages_durable_agent_transcript_with_opaque_cursor` |
| No parallel recorder | Implementation reads the v3 transcript through `SessionStorePort::load_agent` |

## Commands

```bash
cargo test -p piko-protocol rollout_page_get
cargo test -p piko-hostd --test server_jsonl rollout_pages
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
