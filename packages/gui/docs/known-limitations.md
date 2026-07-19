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
- **Chrome presentation.** Vendored Lucide icons, typography roles, and an
  English chrome catalog are landed. Additional locales (for example `zh-CN`)
  and `[gui].locale` remain deferred. See
  [GUI Chrome Presentation](features/chrome-presentation.md).
- **App icon on bare binary.** Custom Dock / Finder icons require `Piko.app`
  from `packages/gui/scripts/bundle-macos.sh`. `cargo run -p piko-gui` keeps the
  generic executable glyph. Red-close and Cmd+Q share the same busy confirm
  path (active turn or unresolved approval). See
  [GUI App Identity & Safe Quit](features/app-identity-quit.md).
- **Settings / Dock.** Settings UI remains deferred after M4. Command Palette
  is available as a Transient overlay (`Cmd+Shift+P`); see
  [GUI Command Palette](features/command-palette.md) and
  [GUI Overlay Stack](features/overlay-stack.md).
- **Approvals on live models.** Deterministic Core/GUI tests cover prompts;
  real-hostd smoke exercises submit/cancel when a model catalog is present, and
  skips that slice without auth.
