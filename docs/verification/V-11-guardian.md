# V-11: F-11 guardian auto-review acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-hostd` domain guardian unit tests (`domain/guardian/mod.rs`,
> `domain/config/settings.rs`), hostd approval-gateway integration tests
> (`adapters/agent_runner/orch_runner/tests.rs`), `piko-orchd` registry decision
> tests (`adapters/tools/registry_tests.rs`), full workspace suite
> Environment: macOS (arm64), `cargo test -p piko-hostd --lib guardian`,
> `cargo test -p piko-orchd --lib guardian`, `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib guardian
cargo test -p piko-orchd --lib guardian
cargo test -p piko-orchd-api
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-11 slice 1 acceptance criteria pass:

- **Settings**: `guardian_settings_merge_field_by_field` merges
  `[guardian]` across base/overrides (override wins per field, missing
  fields inherit); `guardian_defaults_are_documented_in_installed_settings` checks the
  shipped `settings.toml`; `guardian_config_resolves_defaults_and_disablement`
  resolves `enabled=false`/absent → no guardian, defaults 30s timeout and 3
  consecutive denials.
- **Strict JSON**: `parses_strict_allow_and_deny` accepts exact
  `{"allow": bool, "reason": string}`; `rejects_malformed_output` fails
  closed on empty text, prose, wrong `allow` type, missing `allow`,
  non-string `reason`, trailing content, and arrays.
- **One-shot allow**: `guardian_allow_executes_one_shot_without_store_grant`
  returns `Accept` from the gateway and an identical second call is reviewed
  again — no session/workspace/permanent grant was written.
- **Deny fails closed**: `guardian_deny_fails_closed_and_breaker_escalates_to_user_then_resets`
  returns `GuardianDenied { reason }`, and the orchd registry test
  `guardian_denied_decision_fails_closed_with_reason` maps it to a
  non-retryable `guardian_denied` error carrying the reason.
- **Failure/timeout fail closed**: `guardian_failure_fails_closed_without_running`
  and `guardian_timeout_fails_closed` return `GuardianUnavailable` (the
  gateway-level deadline cancels a slow reviewer); the registry test
  `guardian_unavailable_decision_fails_closed` maps it to a non-retryable
  `guardian_unavailable` error.
- **Circuit breaker**: after 2 consecutive denials (threshold), the third
  request reaches the user flow (a pending approval entry exists and
  `respond_approval` resolves it with `Accept`); the following request is
  reviewed again, proving the user decision reset the breaker.
- **Decision mapping**: `expired_is_never_accepted` now asserts
  `GuardianDenied`/`GuardianUnavailable` are never accepted decisions.

## Invariants

- A guardian allow never writes a store grant (one-shot only).
- Guardian denies and failures are terminal, non-retryable tool errors with
  distinct codes (`guardian_denied` / `guardian_unavailable`).
- Timeout/malformed output never auto-approves (fail closed).
- The circuit breaker trips after `max-consecutive-denials` consecutive
  non-accepts and any user decision re-arms it.
- Guardian decisions never create a pending approval or publish an
  `ApprovalRequested` event until the breaker escalates to the user.
