# Session Commit and Reconciliation Boundary

The Host owns the boundary between an orchestrator run and durable session state.

## Commit sequence

1. Load model context from `SessionManager.loadMessages()`. Structural entries
   are adapted for the model at this boundary.
2. Run the orchestrator and forward runtime events to the TUI.
3. Persist the returned message delta.
4. Flush queued persistence work.
5. Expose `loadBranchEntries()` as the authoritative, ID-bearing branch snapshot.

The TUI must receive the snapshot only after steps 3 and 4. This guarantees that
the completion view and a subsequent session resume use the same source data.

The Host does not make UI ordering decisions and the orchestrator does not know
about session tree entry types. This keeps model execution independent from the
storage and presentation schemas.

