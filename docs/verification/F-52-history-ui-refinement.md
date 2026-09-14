# F-52 History UI refinement verification

> Date: 2026-09-06
> Status: partial UI verification; full visual acceptance remains open
> Design: [D-69](../design/D-69-session-history-inspector.md)

## 2026-09-14 stale assertion repair

- The three remaining failing history tests were stale expectations from the
  redesign, not regressions: the retry copy is now "reopen to retry", the wide
  detail-scroll test now focuses the detail pane as the dispatcher does on an
  explicit open, and back on the bare stream closes the surface per D-69 while
  keeping the active session (test renamed
  `back_on_the_stream_closes_and_keeps_the_active_session`).
- `cargo test -p piko-tui history` now passes all 46 tests; `cargo test
  -p piko-hostd` passes; `cargo fmt --all` and
  `cargo clippy --workspace --all-targets -- -D warnings` pass.

## 2026-09-14 interaction performance follow-up

- The user-facing slash command is `/trajectory [session-id]`; `/history` is
  no longer registered in the local command catalog.
- History wheel/trackpad input now scrolls the list viewport by three rows per
  event and does not issue detail requests. Selection follows only when it
  would otherwise leave the visible range.
- The host history cache shares revision-aligned inspection bundles through
  `Arc`; a detail query no longer clones the full current/history/trajectory
  snapshot. The TUI retains up to 32 opened details for the inspected revision.
- Stream row counts and selected-row lookup no longer clone the loaded stream
  during layout preparation. Superseded detail responses are cached without
  briefly replacing the currently requested row.
- Summary now owns journal metadata and no longer repeats the selected row in a
  second `Journal evidence` section. Detail scrolling reuses formatted tab
  lines instead of pretty-printing and wrapping the complete body on each
  pointer event.
- History wheel gestures now remain inside the bounded input batch, so a burst
  of trackpad events produces one paint per cycle instead of one paint per
  event. The stream pane projects only the visible row range during those
  paints.
- Visible stream rows now publish absolute-index hit rectangles in both wide
  and list-only layouts, so clicking a row continues to select and open the
  intended item after scrolling.
- The lane strip and master-detail region are separated by a full-width muted
  border. The list no longer spends a status row on the pagination-only
  `more` label; filtering still reports matched versus loaded rows.
- Summary detail now separates content, relations, journal metadata, and
  snapshot identity into aligned sections with hanging wraps for long IDs.
- ModelStep rows render as ending markers rather than role-badge messages.
  Tool-call Payload and Result tabs reuse the expanded Timeline ToolBlock
  presenter, including tool-specific command/read/edit formatting.

Targeted pointer and detail-cache regression tests passed. The hostd
`session_history` and `session_history_paging` integration tests passed (7
tests), and `cargo clippy -p piko-hostd -p piko-tui --all-targets -- -D
warnings` passed. The ordering, lane-border, filtered-count, and scrolled-row
click regressions also pass. The broader `cargo test -p piko-tui history` run
now passes 40 of 45 tests; the remaining five are pre-existing
presentation/back-navigation assertions, with no new failure from this
follow-up.

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
Filtered:   0 / 30 loaded
Detail:     Detail unavailable: transport failed · open again to retry
            Summary  Step 1 · inspect history rendering / 检查历史记录
            Journal / Position  revision 1 · event 0
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
