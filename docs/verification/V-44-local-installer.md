# V-44: F-33 local installer

> Feature: [F-33](../features/F-33-local-installation.md)
> Design: [D-45](../design/D-45-local-installer.md)
> Verified: 2026-08-13

## Automated evidence

```bash
./scripts/test-install.sh
cargo test -p piko-hostd --lib --test models --test settings --test agent_directed_chat
cargo test -p piko-llmd
cargo test -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p piko-hostd -p piko-tui
```

`scripts/test-install.sh` uses a temporary `PIKO_HOME` and accepts either fake
or release executable artifacts. The verification run used the release
binaries. It verifies executable names/modes, the complete initialized tree,
private auth-file permissions, and byte-preserving config behavior across a
second install. It also verifies idempotent zsh integration, both bash startup
paths, fish `conf.d` integration, and `--no-modify-path` isolation.

The Rust suites verify that provider catalogs parse from filesystem fixtures,
agent definitions load from filesystem catalogs, installed-theme parsing
retains complete semantic slots, and the host/TUI integration remains intact.

The full `cargo test --workspace` run reaches a pre-existing architecture-test
failure because `domain/bookkeeping/occupancy.rs` imports `piko_orchd`; the
focused affected suites above pass, and workspace clippy passes with warnings
denied.

## Static evidence

`rg 'include_(str|bytes)!' packages/hostd packages/llmd packages/tui -g '*.rs'`
returns no matches. Editable settings, agents, model catalogs, and themes are
therefore not compiled into the production host, gateway, or TUI.

## Acceptance mapping

| F-33 criterion | Evidence |
|---|---|
| Executables under the installation `bin` directory | installer integration test |
| Complete editable resource tree | installer integration test and repository resource loops |
| Settings/auth/keybindings initialization and auth permissions | installer integration test |
| Runtime responds to edited files | filesystem-only agent/model/theme loaders |
| Reinstall preserves configuration | sentinel content assertion in installer integration test |
| No embedded editable TOML | static `include_str!`/`include_bytes!` scan |
| Shell PATH integration and opt-out | isolated zsh/bash/fish installer cases |
