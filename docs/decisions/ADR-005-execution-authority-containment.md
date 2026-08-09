# ADR-005: Separate execution authority from containment and process runtime

> Status: accepted
> Date: 2026-08-09

## Context

piko currently uses one sandbox `Policy` for path roots, network posture, a
static executable-name whitelist, shell validation, and workspace-tool safety
evidence. Separately, hostd applies command prefix rules and approval grants.
The resulting `bash` contract advertises shell execution but rejects common
shell constructs before execution, while the executable-name whitelist cannot
contain interpreters or command arguments. When the OS sandbox is disabled,
the same policy shape can appear to constrain effects that are not enforced.

F-23 redefines the command-execution behavior using codex-rs as modeling
evidence. The decision affects the hostd/orchd/sandbox boundaries and the
approval, permission-profile, and tool-system features.

## Decision

- Treat **authorization**, **containment**, and **execution** as separate
  domains:
  - hostd owns user/operator authorization policy, approvals, grants, and
    durable approval state;
  - orchd owns command-attempt orchestration and live process supervision;
  - piko-sandbox owns enforced filesystem/network containment and process
    spawning primitives.
- The model-facing command is a real shell program. Shell parsing is
  best-effort input to authorization and reusable-rule derivation; parse
  uncertainty never rejects otherwise valid shell.
- Command-name or prefix rules are not a containment mechanism. Every allowed
  restricted command, including interpreters and build tools, runs under the
  same enforced permission profile.
- A restricted profile that cannot be enforced fails closed. Direct host
  execution is a distinct elevated mode requiring explicit policy and, when
  applicable, approval.
- Ordinary nonzero child exit codes are execution results. Tool errors are
  reserved for policy, approval, containment, spawn, session, or internal
  failures.
- Preserve dedicated path-aware workspace mutation tools. Their deterministic
  path authorization is stronger than inferring mutation intent from shell.
- Preserve hostd authority: orchd may detect a sandbox denial and request a
  broader attempt, but only hostd may authorize that attempt or persist its
  grant.

## Consequences

- The static shell syntax rejection and executable basename whitelist are
  removed from the execution path rather than expanded.
- piko needs an explicit effective permission profile and typed attempt/result
  contracts instead of one overloaded sandbox policy.
- The OS sandbox becomes required whenever the effective profile is
  restricted; backend capability detection is part of execution readiness.
- Approval prompts become less frequent for work fully contained by the
  active profile, while elevation becomes more explicit and auditable.
- Command policy parsing can evolve independently and conservatively without
  changing which shell programs are valid.
- Existing F-08 process lifecycle code is reusable, but F-08/F-12/F-17
  documentation and verification must be updated as F-23 slices land.
- Compatibility adapters are required for persisted/model histories that
  reference `bash` and `process`.
