// ============================================================================
// SolidJS store wrapper around TuiState
// Bridges the Phase 1 state/reducer with SolidJS reactivity.
// ============================================================================

import { batch, createSignal } from "solid-js";
import type { Model, ModelProviderConfig } from "../../shared/index.js";
import type { TuiEvent } from "../../state/events.js";
import { tuiReducer } from "../../state/reducers/index.js";
import { createDefaultTuiState, type TuiState } from "../../state/state.js";
import { traceDispatch } from "./instrumentation.js";

// ============================================================================
// Store
// ============================================================================

export function createTuiStore(initialState: TuiState) {
  const [state, setState] = createSignal<TuiState>(initialState);

  function dispatch(event: TuiEvent): void {
    traceDispatch(event.type);
    setState((prev) => tuiReducer(prev, event));
  }

  function batchDispatch(events: TuiEvent[]): void {
    batch(() => {
      for (const event of events) {
        dispatch(event);
      }
    });
  }

  return {
    state,
    dispatch,
    batchDispatch,
    setState,
  };
}

export type TuiStore = ReturnType<typeof createTuiStore>;

// ============================================================================
// Factory
// ============================================================================

export function createDefaultStore(
  model: Model<string>,
  providerConfig: ModelProviderConfig,
  cwd: string,
  initialLayout?: Partial<import("../../state/state.js").TuiLayoutState>,
): TuiStore {
  const initialState = createDefaultTuiState(model, providerConfig, cwd, undefined, initialLayout);
  return createTuiStore(initialState);
}
