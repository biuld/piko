import type { TuiEvent } from "./events.js";
import type { TuiState } from "./state.js";

export interface TuiStoreContract {
  state(): TuiState;
  dispatch(event: TuiEvent): void;
  batchDispatch(events: TuiEvent[]): void;
  setState(next: TuiState | ((previous: TuiState) => TuiState)): void;
}
