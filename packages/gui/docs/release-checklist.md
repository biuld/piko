# M4 release checklist

## Automated gates

Passed in the current workspace on 2026-07-18, including an unskipped real-hostd
create/open/reconcile/ListModels smoke. Re-run before release if the tree changes.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p piko-client-core
cargo test -p piko-gui
```

With hostd available, ensure smoke is not skipped:

```bash
cargo test -p piko-gui hostd_smoke -- --nocapture
cargo test -p piko-gui hostd_shutdown_reaps -- --nocapture
```

## Product criteria

- [x] Open/create → reconcile → Live against real hostd
- [ ] ListModels completes; submit → cancel when models/auth allow
- [ ] Center Workbench usable with both sidebars closed (Sessions + Inspector)
- [x] GUI does not spawn orchd; Session durability remains on hostd
- [x] `packages/tui` unchanged by this GUI wave; `cargo test -p piko-tui` passes
- [x] Dependency pins still exact (`gpui = "=0.2.2"`, `gpui-component = "=0.5.1"`)

## Manual (required)

Complete [`manual-ux-checklist.md`](manual-ux-checklist.md) on macOS (keyboard,
IME/CJK, detached streaming, layout breakpoints, notifications).

## Docs present

- [x] [`launch.md`](launch.md)
- [x] [`known-limitations.md`](known-limitations.md)
- [x] [`support.md`](support.md)
- [x] [`dependency-pins.md`](dependency-pins.md)
