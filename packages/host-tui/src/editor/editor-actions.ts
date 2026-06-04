// ============================================================================
// Editor actions — high-level actions emitted by Editor to the outside world
// ============================================================================

export type EditorAction =
  | { type: "submit_prompt"; text: string }
  | { type: "execute_command"; command: string; args?: string }
  | { type: "open_global_surface"; surface: "model" | "settings" | "resume" }
  | {
      type: "global_key";
      event: { name: string; ctrl: boolean; shift: boolean; alt?: boolean; meta?: boolean };
    };
