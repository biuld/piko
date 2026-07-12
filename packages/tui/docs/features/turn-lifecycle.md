# Turn Lifecycle

## Overview

Every accepted prompt has one visible Turn lifecycle. A Turn starts when the
backend accepts its root Agent Execution and ends exactly once as completed,
failed, or cancelled. The running indicator follows that lifecycle rather than
inferring completion from assistant text or runtime availability.

## Layout

Turn lifecycle does not add or move panels. While a turn is active, existing running indicators continue to appear in their current locations. Errors use the existing notification and timeline error presentation.

## Behavior / interactions

- Submitting a prompt creates a distinct Turn and root Agent Execution.
- The running indicator begins when the backend reports that Turn as started.
- A completed, failed, or cancelled event stops the indicator for that Turn.
- A rejected submit command also stops any provisional running state and displays the rejection.
- Assistant, user, and tool messages appear at most once after they are committed.
- Reopening a session restores committed transcript entries without resuming an execution that was interrupted by a previous host process.
- A future detached child Execution does not keep the original Turn running.
  An attached child Execution participates in the root Execution's completion
  barrier.

## Configuration

There are no settings or keybinding changes.

## Non-goals

- This feature does not change panel placement, focus behavior, keyboard shortcuts, or visual styling.
- It does not treat the end of assistant text streaming as proof that a turn succeeded.
- It does not keep a recovered historical turn running when no live backend execution can be proven.
