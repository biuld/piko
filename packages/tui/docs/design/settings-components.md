# Design: Settings component kit

> Status: accepted (implements [settings.md](../features/settings.md))
> Feedback: [component-feedback.md](../features/component-feedback.md)

## Composition

```
SettingSection / Choice
  → SelectableItem (settings_row | settings_option)
  → render_selectable_list_with_pane (product PaneSpec)
  → Pane
```

## Pane chrome (screenshot language)

```text
┌─ Settings                              [x] ─┐
│ / type to filter                             │
│ ─────────────────────────────────────────── │
│ Appearance ──────────────────────────────── │
│ ▸ Compact mode                          off │
│ ▸ Theme                          Grok Night ›│
│ ↑/↓ nav | Enter open | → expand | Esc close │
└─────────────────────────────────────────────┘
```

- Search: `/` glyph + dim `type to filter` (live filter shows `/ <filter>` accent)
- Hairline rule under search
- Title plain (no counter); root shows `[x]`
- Footer: pipe-separated legend; multi-line via `\n`
- Expand: `›` on catalog values
