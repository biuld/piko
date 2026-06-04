// ============================================================================
// SolidJS store wrapper around TuiState
// Bridges the Phase 1 state/reducer with SolidJS reactivity.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { batch, createSignal } from "solid-js";
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
  providerConfig: EngineProviderConfig,
  cwd: string,
): TuiStore {
  const initialState = createDefaultTuiState(model, providerConfig, cwd);
  return createTuiStore(initialState);
}
