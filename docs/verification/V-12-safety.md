# V-12: F-12 write safety assessment acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-hostd` domain safety unit tests (`domain/safety/mod.rs`,
> `domain/config/settings.rs`), hostd approval-gateway integration tests
> (`adapters/turns/orch_runner/tests.rs`), `piko-orchd` registry decision
> tests (`adapters/tools/registry_tests.rs`), `piko-sandbox` policy
> writable-root projection, full workspace suite.
> Environment: macOS (arm64), `cargo test -p piko-hostd --lib safety`,
> `cargo test -p piko-orchd --lib safety`, `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib safety
cargo test -p piko-orchd --lib safety
cargo test -p piko-orchd-api -p piko-sandbox -p piko-protocol
cargo test -p piko-hostd -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-12 slice 1 acceptance criteria pass:

- **Settings**: `safety_defaults_are_documented_in_template` checks the
  shipped `settings.default.toml` documents `[safety]` with
  `auto-approve-workspace-writes = true`;
  `safety_settings_merge_field_by_field` proves override wins and missing
  overrides inherit base values; `SafetyConfig::from_settings` resolves the
  default `true` when the section is absent.
- **Constrained writes auto-approve one-shot**:
  `safety_auto_approves_in_roots_write_one_shot_without_grant` returns
  `Accept` for `edit`/`write` targets inside `/workspace` with no pending
  approval entry published; an identical second request is assessed again
  (no store grant was written). Domain tests
  `in_roots_write_is_auto_approved` and
  `parent_traversal_inside_root_is_still_contained` cover absolute,
  relative, nested-root, and `..`-within-root targets.
- **Out-of-roots writes fail closed**:
  `safety_rejects_out_of_roots_write_with_reason` returns
  `SafetyRejected { reason }` naming the offending path, and the orchd
  registry test `safety_rejected_decision_fails_closed_with_reason` maps it
  to a non-retryable `safety_rejected` error. Domain tests
  `out_of_roots_write_is_rejected_with_reason` cover absolute escapes,
  `..` traversal, sibling-project, and prefix-sibling paths
  (`/Users/biu/Projects/piko-secret` is not inside `/Users/biu/Projects/piko`).
- **Unassessable requests keep the user flow**:
  `safety_without_writable_roots_falls_through_to_user_flow` creates a
  pending approval entry and a user `Accept` resolves it; domain tests cover
  non-write tools, missing/non-string `path`, empty roots, and relative
  targets without a session cwd.
- **Opt-out preserves pre-F-12 behavior**:
  `safety_opt_out_keeps_user_flow_for_in_roots_write` with
  `auto-approve-workspace-writes = false` still publishes a pending approval
  for an in-roots write.
- **Non-write tools unaffected**:
  `safety_never_assesses_non_write_tools` shows `bash` with writable roots
  still reaches the user flow; existing F-07/F-11 gateway and registry tests
  continue to pass unchanged.
- **Decision mapping**: `expired_is_never_accepted` now asserts
  `SafetyRejected` is never an accepting decision.
- **Hardening**: `verify_resolved_accepts_stable_paths_and_detects_swaps`
  (piko-sandbox) accepts a stable in-roots file and rejects the same lexical
  input after it is swapped for an out-of-roots symlink;
  `test_compute_path_fingerprint` (piko-hostd) asserts `edit:src/lib.rs`
  / `write:/abs/out.md` fingerprints and that one path never matches
  another; `write_and_edit_are_denied_inside_dot_piko` (piko-orchd) proves
  `write`/`edit` of `.piko/approvals.json` fail with `access_denied` and
  leave the file untouched; `edit_rejects_empty_old_text`,
  `edit_rejects_non_unique_match_with_line_numbers`, and
  `edit_not_found_message_guides_the_model` cover the tightened edit
  semantics.

## Invariants

- A safety auto-approval never writes a store grant (one-shot only); every
  write is re-assessed.
- Out-of-roots writes are terminal, non-retryable `safety_rejected` errors
  with the offending target in the message.
- The deterministic safety gate runs before the guardian and user flows;
  non-write tools and unassessable requests are untouched.
- The writable-root projection uses the same canonicalization as
  `Policy::authorize`, so the assessment cannot drift from the enforcement.
- Workspace tools cannot write `.piko/` (approvals/settings) under the
  default policy; path-level fingerprints stop cross-path grant leakage;
  writes re-verify the authorized path immediately before writing.
