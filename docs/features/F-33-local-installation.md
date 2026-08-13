# F-33: Local installation

> Status: implemented
> Priority: P0
> Source evidence: piko product direction

## Summary

piko can be installed as a self-contained user installation. Executables and
the editable configuration catalog live under the user's piko directory, so a
user can inspect and change every shipped agent, model, theme, and setting
without rebuilding piko.

## Problem

Today the executable embeds the default settings template, agent definitions,
model catalogs, and themes. This makes the source resources look configurable
while the installed product has no visible authoritative copy. There is also
no repeatable installation entry point that lays out both executables and the
configuration they require.

## User journeys

1. A user runs the installer from a piko source checkout. `piko` and
   `piko-hostd` become available under `~/.piko/bin`, and the complete initial
   configuration tree is created under `~/.piko`.
2. A user edits an installed agent, model, theme, or setting and starts piko.
   The running product uses that edited file without a rebuild.
3. A user runs the installer again after upgrading the checkout. Executables
   are refreshed while existing configuration and credentials are preserved.

## In scope

- A source-checkout installer for the TUI and host daemon.
- User-installation layout rooted at `~/.piko` by default.
- Initialization of settings, credentials, agents, model catalogs, themes,
  keybinding overrides, and empty prompt/skill directories.
- Idempotent PATH integration for supported user shells, with an explicit
  opt-out that leaves shell startup files untouched.
- Runtime loading of shipped configuration from the installed files.
- Idempotent upgrades that preserve existing user-authored configuration.
- An explicit installation-root override for development and verification.

## Out of scope

- Remote binary downloads, release channels, signing, or auto-update.
- Project-local configuration initialization.
- Merging upstream changes into a file the user has edited.
- Session or approval-state migration.

## Behavior and states

- A normal install builds release binaries, installs them atomically enough for
  a local user workflow, and creates missing configuration files.
- Existing configuration, credentials, prompts, and skills are not overwritten.
- Executables are replaced on reinstall.
- Missing or invalid required runtime catalogs produce visible diagnostics; the
  runtime does not silently substitute a compiled copy.
- The installer prints the installation root and a `PATH` instruction.
- For zsh, bash, and fish, a normal install makes the command available to new
  shell sessions without requiring a manual profile edit.

## Acceptance criteria

- [x] `~/.piko/bin/piko` and `~/.piko/bin/piko-hostd` are executable after install.
- [x] Every repository-shipped agent, model catalog, and theme exists as an
      editable file under `~/.piko`.
- [x] `settings.toml`, `auth.json`, and `keybindings.json` are initialized;
      `auth.json` is private to the user on Unix.
- [x] Editing an installed catalog changes runtime behavior without rebuilding.
- [x] Reinstalling preserves an existing configuration file byte-for-byte.
- [x] Production binaries do not embed the editable TOML resources.
- [x] Shell integration is idempotent for zsh, bash, and fish, and can be
      disabled with `--no-modify-path`.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Installation root | `~/.piko`, overrideable with `PIKO_HOME` | Matches existing state/config ownership and makes tests isolated. |
| Executable names | `bin/piko` and `bin/piko-hostd` | Gives users one product command while retaining the host/client split. |
| Upgrade conflict policy | Never overwrite an existing config file | User-authored state remains authoritative. |
| Missing installed config | Diagnose/fail for required catalogs; no compiled catalog fallback | Prevents two competing sources of truth. |
| PATH management | Configure the detected zsh/bash/fish startup files; support `--no-modify-path` | A successful install should produce a usable command while retaining an explicit no-mutation path. |

## Open questions

1. A future release installer may add versioned defaults and an explicit
   three-way upgrade workflow.

## Reference evidence

- `packages/hostd/resources/`
- `packages/llmd/resources/`
- `packages/tui/resources/`
