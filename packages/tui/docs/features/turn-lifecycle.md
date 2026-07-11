# Turn Lifecycle

## Overview

Every accepted prompt has one visible turn lifecycle. A turn starts when the backend accepts its root work and ends exactly once as completed, failed, or cancelled. The running indicator follows that lifecycle rather than inferring completion from assistant text or agent availability.

## Layout

Turn lifecycle does not add or move panels. While a turn is active, existing running indicators continue to appear in their current locations. Errors use the existing notification and timeline error presentation.

## Behavior / interactions

- Submitting a prompt creates a distinct turn even when the same root agent handles multiple prompts.
- The running indicator begins when the backend reports that turn as started.
- A completed, failed, or cancelled event stops the indicator for that turn.
- A rejected submit command also stops any provisional running state and displays the rejection.
- Assistant, user, and tool messages appear at most once after they are committed.
- Reopening a session restores committed transcript entries without resuming an execution that was interrupted by a previous host process.
- Background child work does not keep the original turn running unless the root work is waiting for it.

## Configuration

There are no settings or keybinding changes.

## Non-goals

- This feature does not change panel placement, focus behavior, keyboard shortcuts, or visual styling.
- It does not treat the end of assistant text streaming as proof that a turn succeeded.
- It does not keep a recovered historical turn running when no live backend execution can be proven.
