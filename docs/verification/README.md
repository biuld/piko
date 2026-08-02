# Verification evidence

Acceptance and differential validation evidence for feature PRDs. Each
verification record documents a concrete fixture or scenario, how to reproduce
it, measured results, and the invariants it establishes.

Create records with a `V-NN` identifier (e.g. `V-01-…`) when a PRD acceptance
criterion needs reproducible evidence.

## Record template

```markdown
# V-NN: Title

> Date: YYYY-MM-DD
> Fixture: <fixture description>
> Environment: <hardware / OS / build>

## Reproduction

<commands and steps>

## Result

<measured results or observed behavior>

## Invariants

- <checkable invariant established by this evidence>
```
