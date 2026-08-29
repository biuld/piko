# Agent Todo List Overlay Design

> Status: draft
> Feature: [todo-list.md](../features/todo-list.md)
> Parent: [F-27](../../../../docs/features/F-27-agent-todo-list.md),
> [D-39](../../../../docs/design/D-39-agent-todo-list.md)

## Ownership and flow

```text
host snapshot / TodoListUpdated
            │
            ▼
TodoListsState (by AgentInstance id; projection only)
            │ viewed agent + managed feature gate
            ▼
SurfaceId::Todos → centered modal → TodoPanel
```

Hostd remains authoritative. The TUI stores only the latest projection and a
transient overlay scroll offset.

## Integration

- `LocalCommandId::Todos` maps `/todo` to `SurfaceAction::OpenTodos`.
- Opening resets todo scroll and pushes `SurfaceId::Todos`.
- The surface catalog classifies Todos as `Centered(TodoContent)`,
  `ReadOnlyViewport`, dismissible on outside click.
- Centered size is content-aware and clamped to the terminal.
- `TodoPanel` uses standard Pane chrome, renders progress plus item rows, and
  records the painted maximum scroll.
- Selection/page bindings and wheel gestures update `TodoListsState` scroll.
- No Todo `BandId`, plane `Region`, hit target, offer, grant, separator, or
  collapse state exists.

## Projection

The panel derives counts from the current typed `TodoList`. It preserves item
order, truncates each row to the content width, caps visible rows, and shows
directional overflow counts. Completed content uses the shared completed
styling; in-progress and pending rows use the existing checklist language.

If no visible list exists, the panel renders `No todos for the viewed agent.`
without synthesizing data.

## Verification

- Command catalog and reducer tests cover `/todo` opening.
- Layout/render tests prove Todo is absent from the plane and present in a
  centered modal layer.
- Pointer tests cover overlay wheel scrolling.
- Dock Stack tests prove the registry contains only Boundary, Suggest,
  Guidance, and Composer.
