# Selectors and Model Selector

Selectors must use one shared shell and list implementation. Model/thinking/resume/settings/session-tree/scoped-models should not be separate ad hoc layouts.

## Pi reference

pi uses a shared `SelectList`:

- owns selected index, filtered items, visible window, wraparound navigation, confirm, cancel
- renders with actual width
- lays out primary column plus optional description column
- truncates primary text through policy
- shows description only when width allows
- shows compact scroll state

## Shared selector contract

```ts
interface SelectItem<T = unknown> {
  id: string;
  label: string;
  description?: string;
  value: T;
  disabled?: boolean;
  badge?: string;
}

interface SelectorController<T = unknown> {
  id: string;
  title: string;
  items: () => SelectItem<T>[];
  selectedIndex: () => number;
  filter: () => string;
  setFilter: (value: string) => void;
  move: (delta: number) => void;
  page: (delta: number) => void;
  confirm: () => void;
  cancel: () => void;
}
```

`SelectorController` is a `FocusOwner`.

## Layout budget

```text
selector height
  1 title row
  1 optional filter row
  N list rows
  1 optional scroll/status row
  1 hint row
```

Rules:

- `maxListRows = clamp(visibleHeight - fixedRows, 3, 12)`.
- Render only visible list window.
- Never render list rows into hint row or border.
- Hide descriptions below width threshold.
- Hide filter row for non-filterable selectors.
- Use max width: `min(terminalWidth - 4, 96)`.
- Compact fallback: one-column rows with descriptions omitted.

## SelectListView

Responsibilities:

- render visible window around selected item
- show selected prefix or theme marker
- stable primary and description columns when width allows
- middle-truncate long model ids
- truncate descriptions at right edge
- show no-match state inside list area
- show scroll counter only when filtered items exceed visible rows

## Model selector redesign

Item shape:

```ts
{
  id: `${provider}/${modelId}`,
  label: modelId,
  description: `[${provider}] ${modelName}`,
  badge: isCurrent ? "current" : undefined,
  value: { provider, modelId }
}
```

Filtering:

- match provider
- match model id
- match display name
- rank exact provider/model first
- rank current model first only when filter is empty

Selection:

- `Enter` calls `actionSvc.switchModel(modelId, provider)`.
- On success, close selector and restore editor focus.
- On failure, keep selector open and notify error.

Visual requirements:

- No row crosses surface boundary.
- Selected row may use background color but columns remain readable.
- Current model uses small badge/check marker without shifting primary column width.
- Filter placeholder does not overlap list.
- Hint row is inside the active surface and generated from keymap.
- Narrow terminals collapse to `provider/model` with no description column.

Acceptance criteria:

- `/model` or `Ctrl+L` opens selector with no list/hint/border overlap.
- Typing filters the focused selector; arrows still move selected row.
- Long model ids are truncated within primary column.
- `Esc` restores editor focus.
