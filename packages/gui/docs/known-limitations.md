# Known limitations (M4)

- **macOS-first.** Other platforms are deferred.
- **No orchd client.** GUI talks only to hostd over JSONL stdio.
- **No durable Session ownership.** Transcripts and tree facts come from hostd
  reconcile; GUI keeps drafts, follow, and tree expansion as presentation state.
- **Usage StatusBar.** Protocol `Usage` exposes cumulative tokens/cost, not a
  context `used/limit` window. The bar omits empty values rather than inventing
  a budget.
- **Layout persistence scope.** Split sizes, pane visibility, and reduced-motion
  preference persist through hostd's `[gui]` settings. Island keyboard focus,
  tree expansion, drafts, and per-Agent scroll/follow positions remain
  window-local.
- **Island isolation.** Each island is a GPUI Entity with directed messaging
  (`IslandMsg`). Scroll/hover notify that island only; hostd updates dirty-push
  projections into interested islands rather than broadcasting a full Workbench
  rebuild on every delta.
- **VirtualList.** Timeline uses Scrollable; virtualization waits for measured
  need.
- **Transcript selection/export.** Composer selection and clipboard use the
  native input component; selecting or exporting rendered transcript Markdown
  is deferred.
- **Accessibility.** Visible badges exist; platform VoiceOver announcements are
  not bridged yet.
- **Settings / Palette / Dock.** Deferred after M4.
- **Approvals on live models.** Deterministic Core/GUI tests cover prompts;
  real-hostd smoke exercises submit/cancel when a model catalog is present, and
  skips that slice without auth.

See [`manual-ux-checklist.md`](manual-ux-checklist.md) for IME/focus/pointer gates
that cannot be automated in pure Rust tests.
