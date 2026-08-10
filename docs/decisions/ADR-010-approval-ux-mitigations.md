# ADR-010: Approval-ux mitigations for routine command execution

> Status: accepted
> Date: 2026-08-10

## Context

Real piko sessions showed one approval prompt for nearly every shell command
that touched the standard toolchain: the default macOS seatbelt policy did not
cover Homebrew binaries (`/opt/homebrew/bin/magick`, `convert`), Xcode
CommandLineTools, or read-only `$HOME` config, so ordinary commands
(`git status`, `python3`, image tools) hit platform denials. Per F-23, each
typed denial triggers at most one approval-backed retry, and the
implementation retried with `require_escalated` — which always requires a user
prompt. Grants were fingerprinted on the full command string and retries
carried no reusable prefix, so every new command prompted again. In one
session this produced eleven prompts, and one un-attended approval expired
after the 120s deadline, failing the command.

Two adjacent symptoms surfaced in the same sessions: `exec_command` returned a
`running` result after the 10s default yield for a `find` that walked a 65GB
`target/` tree, and a `todo_write` was rejected atomically because the model
emitted one item without `status`, voiding the whole plan and ending the turn.

## Decision

Keep the F-23 authority model (default sandbox, explicit elevation, one
approval-backed retry, host-owned approvals) and reduce routine friction:

1. **Platform sandbox defaults cover the standard toolchain.** The default
   policy adds read-only access to system toolchains (OS binaries,
   frameworks/dylibs, Homebrew and `/usr/local` bin/lib/Cellar, Xcode
   CommandLineTools) and read-only `$HOME` config, plus writable scratch roots
   for platform temp locations. `$HOME` never becomes a write root.
2. **Denial retries prefer narrow additional permissions.** A typed denial
   first derives the narrowest representable `with_additional_permissions`
   and requests approval for that attempt; `require_escalated` is used only
   when the denial cannot be represented as sandbox permissions.
3. **Eligible approved retries attach a reusable narrow prefix.** Repeat
   commands under an approved prefix reuse the grant instead of prompting
   again. Eligibility matches operator prefix-rule restrictions (no shells,
   interpreters, script runners, destructive utilities).
4. **`exec_command` default yield becomes 30 seconds**, and a `running`
   result explicitly instructs the model to poll with `write_stdin`.
5. **`todo_write` defaults a missing `status` to `pending`**; unknown status
   values still reject with an actionable, item-indexed error.

## Consequences

- Ordinary read/toolchain commands run inside the default sandbox without
  prompting; the approval path becomes the exception rather than the rule.
- Retries stay as narrow as representable, preserving least authority and
  keeping approved retries sandboxed where possible.
- Approval grant reuse is explicit and prefix-scoped, so it cannot silently
  broaden into arbitrary shell or interpreter commands.
- Long commands need fewer model round-trips; the yield contract is stated in
  the result so models stop treating `running` as final.
- `todo_write` becomes lossy-tolerant for missing status while remaining
  strict for invalid values; plan state is not voided by a one-field model
  omission.
- F-07's fail-closed approval expiry is unchanged: expiry remains terminal
  and non-retryable at the runtime level, and the model may issue a fresh
  request.
