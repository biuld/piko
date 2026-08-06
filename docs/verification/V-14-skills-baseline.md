# V-14: F-14 skills discovery and prompt exposure

> Date: 2026-08-06
> Feature: [F-14](../features/F-14-skills-plugins.md)
> Design: [D-14](../design/D-14-skills-baseline.md)
> Fixture: hostd prompt resource integration tests, prompt-domain mention
> tests, and F-03 cache/mention verification
> Environment: macOS (arm64)

## Scope under test

- Project/user skill discovery and nearest-definition precedence.
- Frontmatter arrays/booleans, malformed-file diagnostics, and
  `disable-model-invocation` filtering.
- Metadata-only prompt catalog exposure and CatalogStable cache isolation.
- Explicit `$skill` mention success/error Context behavior inherited from
  F-03/D-27.

## Reproduction

```bash
cargo test -p piko-hostd --test resources
cargo test -p piko-hostd domain::prompts::mentions::tests
cargo test --workspace
```

Related existing records:

- [V-03](V-03-prompt-assembly-fragments.md) — skill catalog prompt block.
- [V-27](V-27-mention-syntax.md) — explicit `$skill` retained Context.
- [V-28](V-28-cache-planning.md) — CatalogStable digest isolation.

## Results

- `load_skills_prefers_project_over_global_visible_format` proves a valid
  project skill loads and formats as model-visible metadata;
  `load_skills_prefers_nearest_definition_with_the_same_name` proves the
  closest cwd-relative definition wins over a same-named ancestor.
- `load_skills_parses_yaml_arrays_booleans_and_reports_malformed_frontmatter`
  proves normalized metadata parsing, fail-soft diagnostics, and model catalog
  suppression for `disable-model-invocation`.
- `snapshots_semantic_context_skills_and_templates` proves skill metadata is
  included in the prompt snapshot without requiring full body inlining.
- Cache-plan tests prove skill catalog edits change the CatalogStable digest
  without changing the ResourceSnapshot digest.
- F-03 mention tests prove `$skill` lookup emits a bounded retained Context and
  unknown/unreadable skills fail soft.
- `cargo test --workspace` passes after the notification-center contract test
  was aligned with the current island dependency.

## Acceptance mapping

| F-14 criterion | Evidence |
|---|---|
| Valid skill metadata and location are visible | hostd `resources` snapshot/format tests |
| Nested/direct definitions and nearest precedence | loader recursion, direct-file parsing test, and `load_skills_prefers_nearest_definition_with_the_same_name` |
| Malformed skills fail soft | malformed-frontmatter resource test |
| Arrays/booleans normalize | `load_skills_parses_yaml_arrays_booleans_and_reports_malformed_frontmatter` |
| Disabled skill hidden from model catalog | same resource test and prompt formatter assertion |
| On-demand read instructions, no body in catalog | prompt snapshot test |
| CatalogStable isolation | F-03/D-28 cache-plan tests and V-28 |
| Explicit `$skill` retained Context | F-03/D-27 tests and V-27 |

## Invariants

- hostd owns skill discovery and the frozen prompt snapshot.
- One invalid skill does not prevent other skills or the turn from loading.
- Skill bodies enter model context only through an explicit read or bounded
  mention expansion.
- Parsed skill model/thinking/tool metadata does not change runtime authority.
