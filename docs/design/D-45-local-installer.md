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
agents from `$PIKO_HOME/agents/spec`, provider/model catalogs from
`$PIKO_HOME/models`, and settings from `$PIKO_HOME/settings.toml`. The TUI
loads named themes from the project catalog first and the user catalog second.
Project-local overrides retain their existing precedence.

Development launchers additionally set the internal `PIKO_DEV_SOURCE_ROOT` to
the active checkout. In that mode, mutable user state (`settings.toml`,
`auth.json`, and sessions) remains under `PIKO_HOME`, while the shipped agent,
model, and theme base catalogs load directly from their package resource
directories in the checkout. This selection is explicit and works for both
debug- and release-profile development builds. Installed binaries do not set
the variable and remain fail-closed against their editable catalogs under
`PIKO_HOME`; a missing installed catalog never falls back to the checkout.

No editable resource is embedded in a production binary. Tests may read
repository fixtures explicitly through `CARGO_MANIFEST_DIR`; runtime resource
selection does not depend on `debug_assertions`.

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
- Path-selection tests verify that development catalogs are independent from
  `PIKO_HOME` and installed catalogs remain rooted there.
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
