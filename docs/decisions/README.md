# Architecture decision records

Use this directory for decisions that affect multiple features or package
boundaries. Feature-local choices belong in the corresponding technical
design.

Create new records from [`_TEMPLATE.md`](_TEMPLATE.md). ADRs are numbered
(`ADR-001`), never deleted, and marked superseded when replaced.

| ID | Decision | Status |
|---|---|---|
| [ADR-001](ADR-001-codex-evidence-prd-first.md) | codex-rs is behavior evidence; PRD-first documentation workflow | accepted |
| [ADR-002](ADR-002-codex-modeling-reference.md) | codex-rs is a modeling reference, not a parity target; conflicts resolve by discussion + industry best practice | accepted |
| [ADR-003](ADR-003-protocol-modeling-acp-reference.md) | piko host–client protocol is product-owned; ACP is a modeling reference (not a wire replacement) | accepted |
| [ADR-004](ADR-004-tui-only-product-client.md) | TUI is piko's only first-party interactive client | accepted |
| [ADR-005](ADR-005-execution-authority-containment.md) | Separate execution authorization, enforced containment, and process runtime | accepted |
