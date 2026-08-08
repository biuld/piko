# Base frame layout (product)

```text
frame
  → split_shell (BottomBar: agent · model · cwd · context · cost)
  → plane = Stream ▾ [Notice] ▾ [Suggest] ▾ Composer
  → modals by SurfaceIntent
       Browse  → CoverBody
       Select  → ComposerBand
       Decide  → Centered
  → solve → paint
```

Agents full UI: `SurfaceId::Agents` (Browse). Compact label: BottomBar `agent` item.

See [`../../AGENTS.md`](../../AGENTS.md) and engine docs under `packages/tui-layout/docs/`.
