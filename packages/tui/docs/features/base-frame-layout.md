# Base frame layout (product)

```text
frame
  → split_shell (BottomBar: agent · model · cwd · context · cost)
  → plane = Stream ▾ Dock Stack grants (Notice? Todos? Suggest?) ▾ Composer
       Dock Stack = features/dock_stack (offer/grant; see dock-coexistence)
       Suggest = / command palette or @ file browser (not a separate modal)
  → modals by SurfaceIntent
       Browse  → CoverBody
       Select  → ComposerBand (height from content-row budget)
       Dock    → ComposerBand (approval / tool workflow replace the composer)
       Modal   → Centered (settings dialog)
  → solve → paint
```

Non-resident plane bands are coordinated by the **Dock Stack** feature
([dock-coexistence.md](./dock-coexistence.md)): providers offer heights; the
stack solves grants so Stream keeps a minimum.

## ComposerBand sizing

Select surfaces declare a **content-row budget** (`SelectBandBudget`), not an
absolute band height:

| Recipe | Content rows | Chrome |
|--------|--------------|--------|
| Minimal stacked list (Models, Auth menu) | `min(items, max) × 2` | 4 (top · search · footer · bottom) |
| Minimal dense list (Agents) | `min(items, max) × 1` | 4 |
| Minimal form (Auth API key) | fixed body lines | 3 (no search) |
| Standard info (MCP) | fixed body lines | 5 |

Compose: `band = chrome + content_rows`, body-clamp, and **floor content to a
multiple of row height** so multi-line List items pack flush to the footer.
Items beyond the budget **scroll** inside the content zone.

Agents full UI: `SurfaceId::Agents` (Select / ComposerBand). Compact label:
BottomBar `agent` item.

See [`../../AGENTS.md`](../../AGENTS.md) and engine docs under `packages/tui-layout/docs/`.
