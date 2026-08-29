# V-61: Cross-process hostd and orchd E2E coverage

> Scope: piko JSONL client boundary, hostd, orchd, durable session storage,
> and the TUI PTY path
> Date: 2026-08-29

## Reproduction

```bash
cargo test -p piko-e2e --tests -- --test-threads=1
cargo test -p piko-tui --test terminal_pty -- --test-threads=1
cargo test -p piko-tui --test terminal_e2e -- --test-threads=1
```

## Coverage

The dedicated `piko-e2e` crate starts its real `piko-e2e-hostd` child for each
test. The fixture injects only the model gateway, so all hostd routing,
orchd runtime, sandbox, approval, persistence, and JSONL framing remain real.

The suite covers:

- command catalog, model/auth/config/MCP/process/agent surfaces;
- session creation, filtering, rename, labels, fork, navigation, deletion,
  import, compaction, restart, and durable history rehydration;
- text and multimodal chat, transcript/read-model/rollout/usage projections;
- workspace read/write/edit, process approval and stop, interaction, todo,
  steering, cancellation, and multi-agent child execution;
- TUI keyboard-to-`chat_submit`/`queue_steer` PTY round trips;
- PTY-visible streaming feedback: `initial response` must render before the
  turn completes; both queue and steer requests are issued while the scripted
  stream is open, and the guidance row must show `1 queued` or `1 steer` within
  one second of the corresponding keypress.

Every process test uses an isolated temporary workspace, session directory,
`PIKO_HOME`, scripted gateway trace, and JSONL event trace. No network provider
or user credential is required.
