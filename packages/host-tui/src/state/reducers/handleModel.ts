// ============================================================================
// Model reducers — model_changed, thinking_level_changed
// ============================================================================

import type { ModelChangedEvent, ThinkingLevelChangedEvent } from "../events.js";
import type { TuiState } from "../state.js";

export function handleModelChanged(state: TuiState, event: ModelChangedEvent): TuiState {
  return {
    ...state,
    model: {
      ...state.model,
      current: event.model,
      providerConfig: event.providerConfig,
    },
  };
}

export function handleThinkingLevelChanged(
  state: TuiState,
  event: ThinkingLevelChangedEvent,
): TuiState {
  return {
    ...state,
    model: { ...state.model, thinkingLevel: event.level },
  };
}
