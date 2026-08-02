# piko documentation

Documentation separates product truth from implementation decisions.

```text
docs/
├── features/    # WHAT: technology-independent behavior contracts (PRDs)
├── design/      # HOW: targeted implementation designs
├── decisions/   # WHY: cross-feature architecture decision records (ADRs)
└── verification # EVIDENCE: acceptance and differential validation results
```

## Feature lifecycle

```text
codex-rs core behavior (reference)
        ↓ extract and simplify
Feature PRD
        ↓ resolve product decisions
Targeted design
        ↓ implement vertical slice
Tests + acceptance evidence
```

codex-rs is **evidence, not specification**. A behavior enters piko only when
the Feature PRD intentionally keeps it. The reference implementation may be
inspected to discover behavior, edge cases, and acceptance fixtures; its
architecture is not translated.

## Infrastructure routing

During design, classify every missing capability:

- Agent-runtime behavior stays in the piko packages (hostd / orchd / llmd /
  sandbox / protocol).
- Reusable desktop UI infrastructure goes to the sibling `island-rs` repo.
- Uncertain capabilities start locally behind a narrow boundary and move only
  after their general contract is understood.
