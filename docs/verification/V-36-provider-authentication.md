# V-36: Provider authentication

> Date: 2026-08-09
> Fixture: Typed provider-auth unit and host/TUI integration tests
> Environment: macOS, Rust 2024 workspace

## Reproduction

```bash
cargo test -p piko-llmd
cargo test -p piko-hostd --test auth --test models
cargo test -p piko-tui auth_selector
```

Before commit, also run:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Result

- OpenAI ID-token account metadata maps to the ChatGPT account header.
- OAuth materialization selects the OpenAI Responses adapter and ChatGPT Codex
  subscription endpoint; API-key resolution remains on the Platform config.
- Runtime resolution occurs before each request and refreshes an expired token
  exactly once under concurrent-safe storage access.
- Expired credentials without a registered flow fail closed.
- A provider-neutral fake OAuth flow refreshes without a provider-name branch
  in storage.
- Model capability projection advertises OAuth only for registered flows, and
  TUI renders an arbitrary advertised OAuth provider.
- Credential files are atomically replaced with mode `0600` on Unix.

## Invariants

- OAuth credentials are never returned by `get_api_key`.
- Shared auth storage contains no provider-name refresh switch.
- Provider authentication controls adapter, endpoint, bearer, and headers as
  one request-auth value.
- Model-input telemetry redacts request headers.
- Device-code polling releases the model registry lock and has a provider
  expiry bound.
