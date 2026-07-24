# Features — capability contracts

**What** this chrome kit provides to consuming applications.

Write from the **app author** perspective: behaviors, contracts, non-goals.
Implementation detail, module paths, and backlog status live in
[`../design/`](../design/) and [`../roadmap/`](../roadmap/).

## Feature index

| Feature | One-line |
|---|---|
| [Archipelago](archipelago.md) | Exclusive full-frame place; body is an island workspace |
| [Island runtime](island-runtime.md) | Isolated islands, focus ownership, directed host messaging |
| [List keyboard](list-keyboard.md) | In-island list/tree cursor; pointer ≡ keyboard intents |
| [Overlay](overlay.md) | Elevated panel chrome, responsive fit, focus open/close |
| [Native Markdown](markdown.md) | Semantic Markdown parsing and theme-aware GPUI document elements |
| [Theme](theme.md) | Shared density, type roles, surfaces, icons |

## Feature doc template

```markdown
# Feature Name

## Overview
(one paragraph — what it is for the integrating app)

## Behavior
(contracts the kit guarantees)

## App responsibilities
(what the consumer must implement)

## Non-goals
(what the kit deliberately does not do)
```

## Related

- Design entry: [design/overview.md](../design/overview.md)  
- Status / planning: [roadmap/README.md](../roadmap/README.md)  
