# Model and thinking selector

> Status: implemented
>
> Design: [model-thinking-selection.md](../design/model-thinking-selection.md)

## Overview

`/model` is one two-stage configuration workflow: first choose a
provider-scoped model, then choose a thinking level supported by that exact
target. The model and thinking level are committed together, so changing a
model cannot accidentally retain an incompatible reasoning effort.

There is no standalone `/thinking` command. Thinking remains visible in the
Bottom Bar and host-backed settings, but the quick selection path always
starts from a model target and its authoritative capability catalog.

## Layout

Both stages use a **ComposerBand** Select surface above the composer:

1. **Model** — filterable provider/model rows with name and auth state.
2. **Thinking** — filterable rows derived from the selected model's
   `reasoningEfforts`, with a short description for each level.

The active value is marked when it is present in the current stage. If the
active thinking level is unsupported by the newly selected model, the first
supported option is selected and the user must still confirm it.

## Behavior / interactions

- `/model`, `Ctrl+L`, or `F3` opens the model stage and refreshes the catalog.
- `Up` / `Down` moves through visible rows; typing filters the active stage.
- `Enter` on a model enters the thinking stage without changing host state.
- The thinking stage contains only the selected target's advertised efforts.
- A target with no advertised reasoning efforts contains only `off`.
- `Enter` on thinking sends one config update containing provider, model, and
  thinking level, then closes the workflow.
- `Esc` in the thinking stage returns to the model stage without applying.
- `Esc` in the model stage closes the workflow without applying.
- Pointer activation follows the same two confirmations as keyboard input.

## Configuration

The final confirmation updates these host-owned settings atomically:

- `default-provider`
- `default-model`
- `default-thinking-level`

Supported thinking values remain `off`, `minimal`, `low`, `medium`, `high`,
`xhigh`, and `max`, but any individual target may advertise only a subset.

## Non-goals

- Guessing capabilities from provider or model names.
- Silently mapping one unsupported effort to another.
- Changing host configuration after only the model-stage confirmation.
- Removing file-based or Settings-based editing of the host defaults.
