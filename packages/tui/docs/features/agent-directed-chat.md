# Agent-Directed Chat

## Overview

The Editor sends a message to the Agent currently shown in Timeline. The main
Agent continues the normal session conversation, while selecting a child Agent
allows a direct follow-up conversation with that Agent.

## Layout

Agent-Directed Chat uses the existing Timeline, AgentPanel, Editor, and
BottomBar. It does not add an overlay or change slot sizes. The highlighted
Agent in AgentPanel identifies the recipient of the next Editor submission.

## Behavior / interactions

- Selecting an Agent replaces Timeline with that Agent's conversation.
- Submitting from the Editor sends the text to the selected Agent.
- Messages and streaming output stay in the selected Agent's Timeline.
- Messages are committed to that Agent's transcript and remain visible after
  reopening the session.
- Submitting to the main Agent retains the existing Turn lifecycle, queue,
  compaction, cancellation, and prompt-resource behavior.
- Submitting to an idle open child Agent starts a new child Agent run.
- A child Agent that is busy or not open reports an error without redirecting
  the message to the main Agent.
- Switching the viewed Agent does not redirect an input that was already
  accepted.

## Configuration

Agent-Directed Chat has no dedicated setting or key binding. It follows the
existing Agent selection and Editor submission controls.

## Non-goals

- It does not turn a child Agent run into a main session Turn.
- It does not automatically reopen a closed child Agent.
- It does not allow one TUI submission to target multiple Agents.
- It does not change Agent hierarchy or authorization rules.
