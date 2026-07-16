# Agent-Directed Chat

## Overview

The Editor sends a message to the Agent currently shown in Timeline. Every
accepted submission creates a Turn for that Agent, whether it is the root or a
child AgentInstance.

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
- Every accepted submission uses the same Turn lifecycle and Agent run API.
- One AgentInstance runs at most one Turn at a time. Additional submissions to
  that Agent are queued in submission order.
- Different AgentInstances in the same Session may run Turns concurrently.
- Cancelling stops the active Turn for the Agent currently shown in Timeline.
- A target that is not open reports an error without redirecting the message.
- Switching the viewed Agent does not redirect an input that was already
  accepted.

## Configuration

Agent-Directed Chat has no dedicated setting or key binding. It follows the
existing Agent selection and Editor submission controls.

## Non-goals

- It does not automatically reopen a closed child Agent.
- It does not allow one TUI submission to target multiple Agents.
- It does not change Agent hierarchy or authorization rules.
