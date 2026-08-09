# ADR-004: TUI-only first-party client

> Status: Accepted
> Date: 2026-08-09

## Context

piko previously shipped both a Ratatui terminal client and a macOS GPUI
desktop client. Maintaining two product surfaces split implementation and
verification effort while the project is prioritizing the terminal workflow.

## Decision

piko supports the TUI as its only first-party interactive client.

- Remove the `piko-gui` crate, its package-local documentation, assets, and
  development scripts.
- Remove the hostd-owned opaque `[gui]` configuration namespace and GUI-only
  communication contracts.
- Keep `piko-client-core` as a headless protocol projection library; it is not
  a commitment to another first-party frontend.
- Continue to keep hostd authoritative for user-visible state and keep the TUI
  connected over the existing JSON-lines protocol.

## Consequences

- Product behavior, package-local UI PRDs, and acceptance work target the TUI.
- Existing user `[gui]` settings become unknown top-level configuration and are
  ignored by hostd's forward-compatible deserializer.
- Reintroducing another first-party frontend requires a new product decision
  and its own PRD-first design work.
