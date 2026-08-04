# V-28: F-03 cache-planning polish

> Feature: [F-03](../features/F-03-prompt-assembly.md) (cache-planning slice)
> Design: [D-28](../design/D-28-cache-planning.md)
> Date: 2026-08-04

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Skills/templates labeled CatalogStable; project ResourceSnapshot | `catalog_stable_scopes_are_labeled_independently_of_project_context` |
| Project vs catalog/tool invalidation independence | `project_and_catalog_invalidations_are_independent` |
| `cache_policy` stamped into plan | `cache_policy_from_resources_is_written_into_the_plan` |
| Assembly version `5` | scopes test assertion |
| RunDynamic still orthogonal to prefix | existing `run_dynamic_*` tests in `resources.rs` |

## Commands

```bash
cargo test -p piko-hostd --test resources project_and_catalog
cargo test -p piko-hostd --test resources catalog_stable
cargo test -p piko-hostd --test resources cache_policy
cargo test -p piko-hostd --test resources run_dynamic
```

## Results

All listed tests pass.

| Test | Result |
|---|---|
| `catalog_stable_scopes_are_labeled_independently_of_project_context` | pass |
| `project_and_catalog_invalidations_are_independent` | pass |
| `cache_policy_from_resources_is_written_into_the_plan` | pass |
| `run_dynamic_fragments_are_deterministic_and_cache_safe` | pass |
| `run_dynamic_environment_does_not_invalidate_stable_cache_prefix` | pass |

## Notes

- `[prompt] cache-policy` is host settings; unit tests set
  `PromptResourceSnapshot.cache_policy` directly (same field submit stamps).
- Multi-breakpoint hierarchical provider caches remain out of scope.
