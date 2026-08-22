# V-58: Exact model-visible tool names

> Date: 2026-08-22
> Fixture: hostd prompt resources and llmd upstream-search resolution tests
> Environment: macOS, Rust test profile

## Reproduction

```bash
cargo test -p piko-hostd --test resources
cargo test -p piko-llmd upstream_catalog_support_still_requires_step_permission
cargo test -p piko-llmd standard_responses_encodes_authorized_upstream_search
cargo test -p piko-orchd registry_resolves_one_sorted_caller_and_upstream_surface
```

## Result

- All 20 hostd resource tests passed, including the platform-policy assertion
  with the todo feature both disabled and enabled.
- Both llmd search-resolution tests passed: the internal upstream kind remains
  `search`, while the authorized model-visible name and Responses definition
  resolve to `web_search`.
- The orchd model-surface regression test passed.

## Invariants

- The structured run tool surface is authoritative for tool identity.
- Models are instructed to use exact supplied names and not invent aliases or
  unavailable tools.
- The behavior correction does not change internal capability kinds, provider
  wire definitions, or execution routing.
