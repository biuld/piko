# Tool Interactive Workflow

## Overview

Tool Interactive Workflow lets the agent pause a running turn to ask the user
for structured input. The request appears as a focused workflow panel in the
chat layout, with one or more questions, selectable choices, optional inline
text input, and an explicit submit step when required.

This feature is for agent-to-user questions that need an answer before a tool
can finish. It is separate from approval prompts, which are only for allowing
or denying a proposed action.

Approval prompts and tool workflows may share the same visual interaction
pattern, but they remain distinct user-facing features: approvals grant or deny
tool execution, while workflows collect answers for a tool that is already
asking the user what to do next.

## Layout

The workflow appears in the Editor slot while the turn is waiting for user
input. It is a partial overlay: the Timeline and AgentPanel remain visible
above it, the Editor is structurally replaced while the workflow is active, and
the BottomBar stays at the bottom.

The panel has a top border and compact content:

- A tab-like header when there are multiple questions
- The active question prompt
- A vertical list of choices
- Inline text input for the selected choice when that choice accepts text
- A short help line with the active keyboard hints

When the workflow has multiple questions and requires explicit confirmation,
the final tab is a Submit step.

## UI Definition

Interactive Workflow is the shared visual pattern for focused, structured user
decisions. Tool workflows and approval prompts use the same panel shape and
interaction vocabulary, while keeping their labels and outcomes specific to the
feature that opened them.

### Panel frame

- The panel occupies the same vertical slot normally used by the Editor.
- It uses a top border only, matching the compact Editor/partial-overlay
  treatment.
- The focused border uses the accent border color.
- Content is inset from the left and right edges so choices do not touch the
  terminal boundary.
- The panel does not draw a floating popup, modal box, or nested card.

### Content order

The panel content is always ordered from navigation context to action:

1. Question tabs, shown only when there is more than one question.
2. The active question line.
3. A blank spacer line.
4. The active question choices, one per line.
5. A blank spacer line.
6. A help line describing the currently valid keys.

When a confirmation step is required, the tab row includes a final Submit tab.
When the Submit tab is focused, the body switches to a short confirmation
message and one focused Confirm action.

### ASCII layouts

Single pending request with one active question:

```text
──────────────────────────────────────────────────────────────────────────────
  Scope: choose how the agent should continue

  ❯ 1. Use the current file only
    2. Search the workspace
    3. Ask me for a custom path

  Enter to select · ↑/↓ to navigate · Esc to cancel
──────────────────────────────────────────────────────────────────────────────
```

Single pending request with multiple questions:

```text
──────────────────────────────────────────────────────────────────────────────
  [Scope]   [Format]   [Submit]

  Scope: choose how much context to use

  ❯ 1. Current file
    2. Open buffers
    3. Whole workspace

  Enter to select · ↑/↓ navigate · Tab switch question · Esc cancel
──────────────────────────────────────────────────────────────────────────────
```

Second question in the same request:

```text
──────────────────────────────────────────────────────────────────────────────
  [Scope]   [Format]   [Submit]

  Format: choose the answer style

    1. Concise
  ❯ 2. Detailed
    3. Custom notes: include risks and test gaps█

  Enter to save · Esc to exit editing
──────────────────────────────────────────────────────────────────────────────
```

Submit step for a multi-question request:

```text
──────────────────────────────────────────────────────────────────────────────
  [Scope]   [Format]   [Submit]

  Ready to submit your answers?

  ❯ [ Confirm ]

  Enter to submit · Tab to cycle · Esc to cancel
──────────────────────────────────────────────────────────────────────────────
```

Multiple pending requests are serialized. The panel shows only the active
request; later requests wait behind it:

```text
──────────────────────────────────────────────────────────────────────────────
  [Approval]                                             2 more prompts waiting

  Approval: run shell command `cargo test -p tui`?

  ❯ 1. Accept once
    2. Accept for session
    3. Accept for workspace
    4. Accept permanently
    5. Decline

  Enter accept once · A session · W workspace · P permanent · Esc decline
──────────────────────────────────────────────────────────────────────────────
```

After the active approval resolves, the next pending request replaces it in the
same slot:

```text
──────────────────────────────────────────────────────────────────────────────
  Branch switch: leave current-path entries behind?

    1. No summary
  ❯ 2. Summarize
    3. Custom prompt

  Enter to select · ↑/↓ to navigate · Esc to cancel
──────────────────────────────────────────────────────────────────────────────
```

### Question line

The question line starts with a short header followed by the prompt. The header
is visually emphasized, and the prompt is normal body text.

Example:

```text
Branch switch: leave current-path entries behind?
```

For approval prompts, the question line describes the proposed action rather
than showing raw protocol fields.

### Choice rows

Choices are rendered as a vertical list:

```text
❯ 1. Summarize
  2. No summary
  3. Custom prompt
```

- The selected row uses the accent color and bold styling.
- Unselected rows use muted styling.
- The selected row starts with the active cursor marker.
- Every row includes a stable numeric shortcut.
- Choice labels should be concise action labels, not explanatory paragraphs.

### Inline input

When the selected choice accepts text, activating it opens inline input on the
same row:

```text
❯ 3. Custom prompt: preserve tool outputs█
```

- Text input appears after the selected choice label.
- A block cursor marks active editing.
- While inline input is active, character keys edit the inline value instead of
  changing panel navigation.
- Leaving inline input keeps the entered value visible in muted styling.

### Tabs and confirmation

For multi-question workflows, tabs show each question header in order:

```text
[Scope]   [Format]   [Submit]
```

- The active tab uses accent color and bold styling.
- Inactive tabs use muted styling.
- Tab and Shift+Tab move between questions and the Submit step.
- The Submit step appears only when the workflow requires explicit
  confirmation.

### Help line

The help line is always the last visible line. It changes with state:

- Normal question: choice navigation, selection, question switching, cancel.
- Inline input: save input and leave editing.
- Submit step: submit and cancel.

The help line is advisory only; it does not introduce extra actions beyond the
active key handling.

### Visual states

| State | UI treatment |
|-------|--------------|
| Focused question | Header and selected choice use accent emphasis |
| Unfocused question tab | Muted tab text |
| Selected choice | Cursor marker, numeric shortcut, bold accent text |
| Unselected choice | Muted text, no cursor marker |
| Inline input active | Text value plus block cursor on the selected row |
| Submit focused | Confirmation message plus focused Confirm action |
| Response sent | Panel remains in place until hostd resolves the request |

### Approval prompt mapping

Approval prompts use this same UI pattern as a single-question workflow:

- The header is Approval.
- The prompt names the tool/action being approved.
- Choices represent approval decisions such as accept once, accept for session,
  accept for workspace, accept permanently, and decline.
- Submitting an approval choice resolves an approval, not a normal tool-input
  answer.

## Behavior / interactions

When a workflow request arrives, focus moves to the workflow panel. The Editor
is hidden until the workflow is answered or cancelled, and its draft content is
preserved for when the workflow closes.

The user can move between choices, enter text for choices that require it,
switch questions, submit the workflow, or cancel it.

Default interactions:

| Key | Action |
|-----|--------|
| Up / Down | Move through choices |
| Number keys | Select a choice by index |
| Enter | Select the current choice, save inline text, or submit when on Submit |
| Tab | Move to the next question or Submit step |
| Shift+Tab | Move to the previous question |
| Backspace | Delete text while inline input is active |
| Esc | Exit inline input, or cancel the workflow when not editing text |

If a request has only one question and no explicit confirmation step, pressing
Enter on a valid choice answers immediately. If the active choice requires
text, Enter first enters text editing, then saves the text, then answers.

When the user submits, the workflow closes and the running turn continues with
the selected answers. When the user cancels, the workflow closes and the tool
receives a cancelled response.

If multiple workflow requests are pending, the TUI shows the oldest request
first. Later requests wait until the active request resolves.

## Configuration

There is no user-facing configuration in the initial version.

Key binding customization may be added later under the existing TUI keybinding
system.

## Non-goals

- This feature does not replace tool approval prompts.
- This feature does not grant permission for sensitive tool execution.
- This feature does not persist answers after the current tool call finishes.
- This feature does not let the user edit previous answers after submission.
- This feature does not expose arbitrary custom widgets from tools.
