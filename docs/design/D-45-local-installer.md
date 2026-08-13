# D-45: Local installer and filesystem configuration

> Status: accepted
> Implements: [F-33](../features/F-33-local-installation.md)
> Decisions: [ADR-016](../decisions/ADR-016-installed-config-authority.md)

## Goal

Deliver a source-checkout installer that places the client and host daemon in
`~/.piko/bin` and materializes all user-editable product configuration under
the same root. Runtime catalog loaders consume those files instead of compiled
TOML strings.

## Constraints and non-goals

- hostd remains authoritative for durable user-visible state.
- The TUI remains a standalone stdio client and locates its sibling hostd.
- Existing user configuration and credentials must survive reinstall.
- Remote distribution, signatures, and automatic shell configuration are not
  part of this slice.

## Proposed design

`scripts/install.sh` resolves `PIKO_HOME` (default `$HOME/.piko`), builds the
release TUI and hostd, then installs them as `bin/piko` and `bin/piko-hostd`.
It creates the catalog directories and copies repository resources only when
the destination file does not exist. Credentials are initialized to an empty
JSON object with Unix mode `0600`; other files are user-readable/writable.

The installer writes a small `$PIKO_HOME/env` PATH fragment and, unless
`--no-modify-path` is supplied, adds one marked, idempotent loader to the
detected shell configuration. zsh uses `.zshrc`; bash covers `.bashrc` and
`.bash_profile`; fish uses `~/.config/fish/conf.d/piko.fish`. Unknown shells
receive printed manual instructions without guessed profile mutation.

Runtime path resolution uses the same `PIKO_HOME` contract. hostd loads global
agents from `$PIKO_HOME/agents`, provider/model catalogs from
`$PIKO_HOME/models`, and settings from `$PIKO_HOME/settings.toml`. The TUI
loads named themes from the project catalog first and the user catalog second.
Project-local overrides retain their existing precedence.

No editable resource is embedded in a production binary. Debug/test builds may
read repository fixtures from `CARGO_MANIFEST_DIR` when no installation exists;
that branch is excluded from release builds and does not create a production
fallback.

## Package impact

| Package | Change |
|---|---|
| `piko-hostd` | Stop creating settings from an embedded template; load installed agents/models. |
| `piko-llmd` | Make provider registries catalog-neutral and parse filesystem TOML. |
| `piko-tui` | Resolve themes from project/user files rather than embedded TOML. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Build failure leaves the existing installation untouched.
- A binary copy failure stops the installer with a non-zero status.
- Shell startup edits are append-only, marker-guarded, and skipped completely
  when requested.
- A missing optional override file is ignored; a required base theme or agent
  catalog reports a clear installation error rather than using compiled data.
- Existing config files are never partially rewritten because normal reinstall
  does not write them.

## Verification

- Shell integration test installs into a temporary `PIKO_HOME`, verifies the
  tree and permissions, checks zsh/bash/fish and opt-out behavior, modifies a
  config, and verifies reinstall preservation.
- Loader unit tests use temporary filesystem catalogs.
- `cargo test --workspace`, formatting, and clippy validate integration.

## Alternatives considered

- Keep compiled defaults as fallbacks: rejected because edits and provenance
  would have two authorities.
- Overwrite configs on every upgrade: rejected because it destroys user state.
- Install only binaries and let first run create resources: rejected because
  TUI and hostd would each need packaging logic and partial-install recovery.

## Rollout

1. Add the installer and installation fixture test.
2. Move host/model/theme loaders to the installation tree.
3. Remove production `include_str!` usage for editable resources.
4. Verify reinstall and workspace tests.
