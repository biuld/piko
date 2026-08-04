# V-27: F-03 mention-syntax parsing

> Feature: [F-03](../features/F-03-prompt-assembly.md) (mention-syntax slice)
> Design: [D-27](../design/D-27-mention-syntax.md)
> Date: 2026-08-04

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Parse `@path` / `$skill` in order; dedupe | `hostd` unit `mentions::tests::parses_file_and_skill_mentions_in_order` |
| Skip email-like `@` and common `$env` | `mentions::tests::skips_email_like_and_env_vars` |
| File under cwd injects body Context | `mentions::tests::resolves_file_under_cwd` |
| Path outside cwd is fail-soft error | `mentions::tests::refuses_path_outside_cwd` |
| Durable chain world → completion → mention → input | `orchd` `inter_agent_completions_chain_after_world_state_before_input` |
| Protocol body format stable | `piko-protocol` `user_mention::tests` |

## Commands

```bash
cargo test -p piko-protocol --lib user_mention
cargo test -p piko-hostd mentions
cargo test -p piko-orchd inter_agent_completions_chain
```

## Results

All listed tests pass.

## Notes

- User message text is not rewritten on submit; bodies land as retained
  Context prelude messages only.
- Plugin / linked markdown mention forms remain out of scope.
