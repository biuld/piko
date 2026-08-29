# piko end-to-end tests

`piko-e2e` exercises the real JSON-lines boundary in a separate hostd process.
The harness starts the `piko-e2e-hostd` binary from this crate, which composes the production hostd
protocol server, durable JSONL session storage, and a deterministic inference
gateway. Tests therefore cover command framing, hostd routing, orchd execution,
filesystem/process effects, and durable projections without network credentials.

Run the suite serially:

```bash
cargo test -p piko-e2e --tests -- --test-threads=1
```

The TUI-level PTY tests remain in `piko-tui` and exercise keyboard input through
the same helper hostd process:

```bash
cargo test -p piko-tui --test terminal_e2e -- --test-threads=1
```

Those tests also assert the visible `initial response`, `1 queued`, and
`1 steer` guidance markers. Queue/steer feedback has a one-second deadline from
the corresponding PTY keypress, and the initial response must arrive before
the scripted turn completes.

The gateway fixture is intentionally scripted. Each test selects a mode through
`PIKO_TUI_E2E_MODE`; no real provider calls are made, and each test receives an
isolated workspace, session root, `PIKO_HOME`, and trace files.
