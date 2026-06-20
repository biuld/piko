# Selectors

Selectors use shared primitives for list rendering, filtering, and keyboard
navigation. Model, thinking, resume, settings, provider, and session-tree
selectors share these primitives while each provides its own data source and
confirmation behavior.

## Shared primitives

### SelectListView

`SelectListView` (in `src/renderer/opentui/select/SelectListView.tsx`) is the
shared list renderer:

- Renders a visible window around the selected item.
- Shows selected prefix or theme marker.
- Supports primary + optional description columns.
- Middle-truncates long IDs.
- Truncates descriptions at right edge.
- Shows no-match state inside the list area.
- Shows scroll counter when filtered items exceed visible rows.

### Selector controller

`selector-controller.ts` provides shared selection state management:

```ts
interface SelectorController<T> {
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

### Selector layout

`selector-layout.ts` computes row budgets and column widths based on viewport
and content:

```ts
function computeSelectorLayout(params: SelectorLayoutParams): SelectorLayout;
```

Rules:
- `maxListRows = clamp(visibleHeight - fixedRows, 3, 12)`.
- Render only visible list window.
- Hide descriptions below width threshold.
- Hide filter row for non-filterable selectors.
- Max width: `min(terminalWidth - 4, 96)`.
- Compact fallback: one-column rows.

## Built-in selectors

### ModelSelector

Located at `src/renderer/opentui/select/ModelSelector.tsx`.

- Opens via `/model` or `Ctrl+L`.
- Item shape: `{ id: "provider/modelId", label: modelId, description: "[provider] modelName" }`.
- Filtering: matches provider, model id, and display name.
- Current model shown with badge marker.
- `Enter` calls `actionSvc.switchModel(modelId, provider)`.
- On success: close, restore editor focus.
- On failure: keep selector open, notify error.

### ThinkingSelector

Located at `src/renderer/opentui/select/ThinkingSelector.tsx`.

- Opens via `/thinking`.
- Options: off, low, medium, high.
- `Enter` calls `actionSvc.setThinkingLevel(level)`.

### ResumeSelector

Located at `src/renderer/opentui/select/ResumeSelector.tsx`.

- Opens via `/resume`.
- Lists recent sessions for resumption.
- Full placement (replaces timeline).

### SettingsSelector

Located at `src/renderer/opentui/select/SettingsSelector.tsx`.

- Opens via `/settings`.
- Hierarchical menu with nested routes via panel route stack.
- Sub-pages: Models, Theme, Thinking, etc.

### ProviderSelector

Located at `src/renderer/opentui/select/ProviderSelector.tsx`.

- Opens from model selector or settings.
- Lists available providers.

### TreeSelector

Located at `src/renderer/opentui/select/TreeSelector.tsx`.

- Opens via `/tree` or double-escape.
- Session tree navigation with branches.
- Flattened tree items with indentation.
- `Enter` confirms selection → `SessionActions.navigateTree()`.

## Panel integration

Selectors are rendered inside panel surfaces. The panel system provides:

- Route stack: push child routes (e.g., Settings → Models).
- Filter bar: `PanelCapability.filter`.
- Hint bar: derived from interaction type + keymap.
- Title: from `PanelChrome.title`.

Selector components receive panel state as props (filter text, selected index,
items) and dispatch panel actions (update_filter, update_selection, confirm,
cancel) via the surface key controller.

## Keyboard behavior

Selectors use `PanelInteraction: "list"` or `"menu"`:

| Key | Action |
|---|---|
| `↑` / `↓` | Move selection up/down |
| `PageUp` / `PageDown` | Move by page |
| `Enter` | Confirm selection |
| `Esc` | Cancel / close |
| Printable | Update filter (if filter capability present) |

## Narrow terminal behavior

- Descriptions hidden below width threshold.
- One-column layout instead of primary + description.
- Reduced max list rows in minimal mode.
