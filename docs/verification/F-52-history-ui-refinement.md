# F-52 History UI refinement verification

> Date: 2026-09-06
> Status: partial UI verification; full visual acceptance remains open
> Design: [D-69](../design/D-69-session-history-inspector.md)

## Follow-up changes

- Lens tabs share their prepared paint/hit rectangles and allocate width by
  label length. At 40 terminal columns, all four names remain readable.
- Filtering shows matched / loaded rows within the current lens and provenance
  scope. Journal counts include commit-header rows; these are presentation rows,
  not whole-session event totals. Paging and loading indicators precede the
  optional provenance description.
- Loading and failed detail retain the opened row and its available relations.
  Feedback wraps in the independent detail viewport, with retry copy before
  technical context. Rows without a detail body offer summary/back guidance.
- Detail viewport painting is separated into `history/detail.rs`; the shared
  split-pane geometry and host query authority remain unchanged.

## Regression evidence

`PIKO_HISTORY_QA_DIR=/tmp/piko-history-qa cargo test -p piko-tui history`
passed 44 tests. Coverage includes the four new regressions for compact tab
labels, filtered loaded counts, wrapped detail feedback with opened identity,
and summary-only action guidance. Existing tests cover pane-specific wheel
routing, scroll/back/resize restoration, prepared pointer geometry, typed
content, all prompt blocks, command correlation, revision invalidation, and
active-session isolation.

`cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
passed. The escalated `cargo test --workspace` run completed successfully,
including TUI, hostd, journal recovery/replay, cross-process tests, and doc tests.

## Rendered buffer inspection

The fixture command above exports 17 cell buffers: each lens at 40, 60, and
120 columns, plus wide/compact/scrolled detail, detail error, and filtered empty
states. Text projections of these buffers were inspected; this is not a live
terminal screenshot or an interactive pointer/hover acceptance run.

Observed examples:

```text
40 columns: [Work] Agents  Transcript  Journal
Filtered:   0 / 30 loaded · more · facts + diag
Detail:     Detail unavailable: transport failed · open again to retry
            Summary  Step 1 · inspect history rendering / 检查历史记录
            Journal position  revision 1 · event 0
```

Fixtures exercise long bodies, CJK summaries, deep transcript indentation,
unavailable diagnostics, commit boundaries, and compact summary inspection.
Remaining visual acceptance includes hover/focus transitions in a live terminal,
legacy-origin and unknown-kind visual fixtures, and narrow row-priority review.
F-52's broad UI acceptance criteria remain unchecked pending that coverage.

## Workspace execution environment

The first sandboxed workspace run stopped in two hostd OAuth callback tests:
localhost listener creation returned `Operation not permitted`. The workspace
suite was rerun with sandbox escalation so local callback listeners could bind.
This environment failure did not require a source change.

The escalated workspace run exited with code 0. No tests reported failure or
being ignored in its standard Rust test summaries.
