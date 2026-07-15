# Session View Lifecycle

## Overview

The session view keeps the timeline, agent list, turn indicator, approvals, and
interactions aligned with the session currently selected in the TUI. Loading,
empty, live, and error states are shown explicitly so stale data from another
session is never presented as current.

## Layout

The feature uses the existing Timeline, AgentPanel, prompt panels, notification
row, and bottom status bar. It does not add a new panel or change slot sizes.

## Behavior / interactions

- During startup, the AgentPanel shows loading until required shell resources
  have completed loading.
- A cold start with no selected session then shows an empty AgentPanel and an
  empty Timeline while leaving the editor available.
- Creating, opening, or switching sessions shows loading until the complete
  session view is ready.
- Text entered while a session is being created or opened is retained and is
  submitted only after that session becomes live.
- Refreshing or reconnecting replaces the visible session view atomically and
  restores any in-process approval or interaction prompt.
- Events belonging to another session do not change the visible timeline,
  agents, prompts, queue status, or turn indicator.
- Deleting the visible session clears all session-owned panels and returns the
  TUI to the no-session state.
- If session creation or opening fails, loading ends and an error notification
  is shown. A previously live session is restored when possible.

## Configuration

There is no configuration for session view lifecycle behavior.

## Non-goals

- Recovering realtime assistant drafts after refresh or reconnect
- Recovering pending approval or interaction prompts after hostd exits
- Changing session storage layout or agent execution behavior
- Persisting editor, focus, scroll, or overlay state
