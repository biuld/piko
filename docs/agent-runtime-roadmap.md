# Agent Runtime Roadmap

> Date: 2026-08-02
> Basis: [codex-rs Agent Core Digest](codex-agent-core-digest.md)
> Workflow: PRD-first (ADR-001) — each block splits into Feature PRDs
> (`docs/features/F-NN`), then designs (`docs/design/D-NN`), then vertical
> slices, then verification evidence (`docs/verification/V-NN`).

## 1. Roadmap shape

The codex-rs agent core splits into 13 functional blocks (A–M). piko already
carries substantial runtime infrastructure, so the roadmap is not a greenfield
port: it is organized as milestones that each make one *capability* whole,
reusing what already exists.

```text
M0 Core loop & foundations   (mostly landed; close remaining gaps)
 └─ A turn-runtime · B model-gateway · C prompt-assembly · E tool-system
    · F approvals · H persistence · I multi-agent (core)
M1 Context & memory          (make long sessions correct)
 └─ D context-management · D compaction · L observability basics
M2 Execution & sandbox       (real work execution)
 └─ G exec/sandbox
M3 Trust & safety            (human-in-the-loop at scale)
 └─ F guardian · F elicitation · F safety assessment · M permissions
M4 Ecosystem                 (skills, plugins, MCP, hooks)
 └─ J skills/plugins/MCP/hooks
M5 Multi-agent depth         (v2 collaboration surface)
 └─ I followup/interrupt/list/wait · agent roles · inter-agent fragments
M6 Observability & ops       (production readiness)
 └─ L rollout/turn-timing/diff-tracking
M7 Extension                 (optional, not core)
 └─ K realtime/multimodal
```

## 2. Milestone details

### M0 — Core loop & foundations

Goal: a turn is durable, resumable, deterministic, and can execute any tool.
Status: the largest part is landed (F-01, F-02 slice, F-06, F-07, F-09,
F-10 core, F-03 baseline). Remaining slices:

| Feature | Slice | Status |
|---|---|---|
| F-06 tool-system | parallel batch dispatch + sequential exclusivity | implemented (D-06, V-06) |
| F-01 turn-runtime | input admission; typed background tasks; turn-abort markers | implemented (D-01, V-01) |
| F-02 model-gateway | retry/backoff budget; streaming fallback | implemented (F-02/D-02/V-02) |
| F-03 prompt-assembly | fragment catalog breadth (world state, environments, model-switch) | implemented (F-03/D-03/V-03) |
| F-10 multi-agent | v2 collaboration tools (followup/interrupt/list/wait) | implemented (F-10/D-10/V-10) |
| F-07 tool-approvals | approval timeout/deny semantics polish | planned |

Exit criteria: every M0 feature has a reviewed PRD; tool batches are parallel;
turn aborts reconstruct durable history; approvals are exercised end-to-end.

### M1 — Context & memory

Goal: long sessions stay correct and within budget without manual intervention.

| Feature | Slice | Status |
|---|---|---|
| F-04 context-management | transcript normalization/truncation; snapshot sharing; token accounting | planned |
| F-05 compaction | auto-compact trigger; inline compact; budget windows; remote compaction | partial (hostd summarizer) |
| F-15 observability | usage accounting per turn (baseline for budget decisions) | partial |

Dependency: M1 needs M0 F-01 (durable transcripts) to be solid before
rewriting history safely.

### M2 — Execution & sandbox

Goal: real commands run with PTY, timeouts, process groups, cancellation, and
fail-closed sandboxing.

| Feature | Slice | Status |
|---|---|---|
| F-08 exec-sandboxing | PTY/process-group lifecycle; shell snapshots; unified long-lived processes; network sandbox; environment selection | partial (file policy + runner) |

Dependency: M2 builds on F-06 tool routes (shell tool) and M0 F-01
cancellation.

### M3 — Trust & safety

Goal: on-request approvals scale without weakening safety.

| Feature | Slice | Status |
|---|---|---|
| F-11 guardian | auto-review loop; compact review transcript; circuit breaker | planned |
| F-12 safety | patch-safety assessment; elicitation pause; attestation | planned |
| M-config | permission-profile materialization; managed features; agent roles | planned |

### M4 — Ecosystem

Goal: MCP, skills, plugins, and hooks are first-class capabilities.

| Feature | Slice | Status |
|---|---|---|
| F-13 mcp-integration | MCP resources/search; approval templates; prewarm | partial (stdio provider) |
| F-14 skills-plugins | implicit skill invocation; plugin discovery/injection | partial (loader + injection) |
| F-14 / new | hooks (additional context, input inspection) | planned |

### M5 — Multi-agent depth

Goal: parent agents can supervise and steer children (codex-rs v2 surface).

| Feature | Slice | Status |
|---|---|---|
| F-10 multi-agent | `followup_task`, `interrupt_agent`, `list_agents`, `wait_agent` | implemented (F-10/D-10/V-10) |
| F-10 multi-agent | agent roles (role config layers) | planned |
| F-03 prompt-assembly | inter-agent notification / completion fragments | planned |

### M6 — Observability & ops

| Feature | Slice | Status |
|---|---|---|
| F-15 observability | end-to-end tracing (OTel spans turn → agent → model → tool; OTLP HTTP + JSON fallback) + TTFT/TTFM metrics; rollout recorder; turn-diff tracking; prompt debugging | implemented (F-15/D-15/V-15; tracing + metrics slice) |

### M7 — Extension (optional)

| Feature | Slice | Status |
|---|---|---|
| F-16 realtime (new) | realtime sessions, multimodal prep | not started — deferred |

## 3. Per-block feature decomposition

Each block splits into Feature PRDs as follows. The `F-NN` IDs are the stable
identifiers used by `docs/features/`; one PRD per row below (row = one
behavior contract with its own acceptance criteria).

| Block | F-ID | Decomposed features (PRDs) |
|---|---|---|
| A Turn & Agent Runtime | F-01 | turn lifecycle; input admission/queueing; background tasks; turn-abort reconstruction |
| B Model Gateway | F-02 | provider registry; streaming + retry; usage/token metadata |
| C Prompt Assembly | F-03 | fragment system; AGENTS.md; skills/plugins injection; world-state/env fragments |
| D Context & Compaction | F-04 / F-05 | transcript manager; truncation; auto-compact; remote compact |
| E Tool System | F-06 | registry/routing; batch dispatch (parallel); approvals hook; dynamic tools; tool search |
| F Approvals & Safety | F-07 / F-11 / F-12 | approval tiers; network approval; guardian; elicitation; safety assessment |
| G Exec & Sandbox | F-08 | process lifecycle; shell; unified exec; platform sandboxes; network policy |
| H Persistence & Resume | F-09 | thread store; fork/resume; rollout files; interrupted markers |
| I Multi-Agent | F-10 | agent tree; v2 tools; roles; inter-agent messages |
| J Skills/Plugins/MCP | F-13 / F-14 | skills; plugins; MCP servers/resources; hooks |
| K Realtime | F-16 | realtime sessions; multimodal prep (deferred) |
| L Observability | F-15 | turn timing; rollout recorder; diff tracking |
| M Config & Permissions | F-12 part | config layers; permission profiles; managed features; agent roles |

## 4. Sequencing principles

1. **Finish M0 before M1+**: context rewriting and compaction assume durable,
   deterministic transcripts (F-01) and complete tool semantics (F-06).
2. **PRD before design, design before code** — one vertical slice per PRD,
   with `docs/verification/V-NN` evidence tied to acceptance criteria.
3. **Differential validation against codex-rs tests**: each PRD's acceptance
   criteria cite the codex-rs fixtures in the digest block.
4. **Keep hostd authoritative**: user-visible state (sessions, turns,
   approvals) stays in hostd; orchd remains transient execution.

## 5. Next step

**M0** now has one remaining slice: **F-07 tool-approvals** (approval
timeout/deny semantics polish). PRD-first it next — draft the F-07 PRD in
`docs/features/`, design in `docs/design/`, then implement. Follow-on M0 gaps
worth sequencing: F-03 mention-syntax parsing and cache-planning polish,
then F-04/F-05 (M1) with world-state diffing building on the frozen
`state.run` baseline.
