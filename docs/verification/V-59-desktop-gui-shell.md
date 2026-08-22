# V-59: Desktop GUI shell verification

> Status: automated evidence complete; visual acceptance pending
> Date: 2026-08-22
> Verifies: [F-42](../features/F-42-desktop-gui-shell.md),
> [D-59](../design/D-59-desktop-gui-shell.md)
> Environment: macOS, Rust workspace debug profile

## Automated evidence

- `piko-desktop` builds as a workspace binary and launches with the local
  `piko-hostd` binary.
- Desktop unit tests cover connection transitions, target-keyed Timeline
  states, stable normalized row identity, negative GPUI tail-offset handling,
  Composer growth bounds, Enter/Shift+Enter submission policy, sidebar
  breakpoint and keyboard reveal, focus restore, primary-surface traversal,
  preferences round-trip/safety clamping, and transport observations.
- The recorded `tests/fixtures/bootstrap.jsonl` stream passes through the
  production `piko-comms::decode_host_line` decoder and the client-core reducer.
- `piko-comms` owns the shared bounded JSON-lines process bridge; both TUI and
  desktop bind it to frontend-specific communication contracts.
- Every desktop Rust source file remains below the 500-line ceiling.

## Commands

```bash
cargo fmt --all
cargo test --workspace
cargo test -p piko-desktop -p piko-comms
cargo check -p piko-tui -p piko-comms
cargo clippy -p piko-desktop --all-targets --no-deps -- -D warnings
cargo clippy -p piko-comms -p piko-tui --all-targets --no-deps -- -D warnings
cargo build -p piko-hostd -p piko-desktop
./target/debug/piko-desktop --hostd-command ./target/debug/piko-hostd
```

## Result

The desktop/comms tests, checks, package-local clippy gates, build, and launch
pass. The full workspace test reaches and passes the desktop suite (35 tests),
then fails in the unrelated, pre-existing `piko-session-store` test
`checksum_verifies_original_float_spelling_without_json_round_trip`; an exact
standalone rerun fails the same way. The workspace-wide clippy gate is also
blocked before reaching desktop code by the pre-existing `piko-protocol`
`clippy::large_enum_variant` diagnostic for `TrajectoryRecord`; package-local
`--no-deps` clippy gates pass.

The environment did not grant screen-capture access, so visual inspection of
the wide and narrow layouts could not be recorded. F-42 remains partial and
its visual acceptance checkboxes remain open until that evidence is gathered.

## Pending visual scenarios

1. Wide window: one detached sidebar, one Timeline, one bottom-floating
   Composer, and no third column.
2. Narrow window: persistent sidebar leaves the island frame; Show Sidebar
   opens the same source list as a temporary layer; selection and Escape close
   it.
3. Multiline Composer: growth stops at eight rows, internal scrolling begins,
   and the last Timeline row remains fully reachable.
4. Streaming while reading: scroll position remains stable and the Latest
   affordance returns to the tail.
5. Model/thinking/attention layers: Escape and backdrop dismissal restore the
   initiating focus owner without clearing the draft.
6. Restart: safe window/sidebar preferences restore, then the last session is
   reopened only after host discovery confirms it.

## Invariants established

- Host-authored `ClientState` remains the sole product projection store.
- Session/agent switches never relabel stale Timeline rows as the requested
  target; agent changes show a shell-local loading guard until correlated host
  acknowledgement.
- Successful submission clears only the submitted draft after correlated host
  acceptance; failure and disconnect preserve recoverable text.
- Realtime and committed assistant rows derive the same message-keyed element
  identity.
- Window/sidebar preferences and last-session warm-open hints remain local and
  cannot override host discovery or reconciliation.
