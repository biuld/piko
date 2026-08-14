# Guidance Row Design

## Selected Feature

This design implements [guidance-row.md](../features/guidance-row.md).

## Ownership

```text
NotificationCenter ───────┐
AutoComplete / Surface ───┼→ Guidance projection → Region::Guidance → dock_line
Composer fallback ────────┘

Dock Stack: Todos? → Suggest? → Guidance(1) → Composer
```

`features::guidance_row` owns content resolution and paint. Dock Stack owns
only the resident one-row grant and ordering. Feature state remains with its
existing owner.

## Layout migration

- Replace `BandId::Notice` with `BandId::Guidance`.
- Register Guidance as a resident, protected one-row anchor directly before
  Composer.
- Remove notice offers from `layout::collect_dock_offers`; Guidance always
  offers preferred/minimum height one.
- Replace `Region::Notice` with `Region::Guidance`.
- Keep `HitId::Notice` as the semantic action when Guidance currently paints a
  notice.
- ComposerBand modal trees contain `Guidance(1) → Surface`, and their host
  height includes both. This keeps Guidance immediately above variable-height
  Select/Dock replacements instead of letting the overlay cover the plane row.

## Hint projection

The resolver returns owned display text because workflow help may be assembled
from dynamic steps. It selects hints only for:

- Chat / Suggest;
- Select ComposerBand surfaces;
- Dock ComposerBand surfaces.

Those surfaces stop reserving a pane footer. Their content-row budgets remove
one footer chrome row. CoverBody and Centered panes keep their existing local
footer projection.

## Paint and hit-testing

- Notice: existing severity spans, hover background, and dismiss action.
- Hint: shared `dock_line::hint_line` paint.
- Empty fallback: clear/passive row with no hit region.
- Guidance remains a plane region, so existing modal z-order prevents clicks
  from falling through an active surface.

## Verification

- Idle, Suggest, Select, and Dock frames grant Guidance exactly one row.
- Notice and hint transitions preserve Stream/Composer geometry.
- Notice pointer dismissal still resolves through `HitId::Notice`.
- Select/Dock panes no longer reserve or paint local hint footers.
- CoverBody/Centered panes retain their local hints.
