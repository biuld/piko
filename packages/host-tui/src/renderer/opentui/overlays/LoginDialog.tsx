// ============================================================================
// Login Dialog Overlay — API key input
// ============================================================================

import { createSignal } from "solid-js";
import type { TuiStore } from "../store.js";
import { OverlayContainer } from "./OverlayContainer.js";

export interface LoginDialogProps {
  store: TuiStore;
  provider: string;
  onClose: () => void;
}

export function LoginDialog(props: LoginDialogProps) {
  const { store, provider, onClose } = props;
  const [apiKey, setApiKey] = createSignal("");

  function handleSubmit(): void {
    const key = apiKey().trim();
    if (!key) return;

    // Dispatch model changed with the new API key
    const current = store.state().model;
    // Note: API key storage is handled by the host/auth layer.
    // For now, this is a placeholder.
    onClose();
  }

  return (
    <OverlayContainer kind="login" title={`Login: ${provider}`} onClose={onClose}>
      <box flexDirection="column">
        <text fg="#d4d4d4">Enter API key for {provider}:</text>
        <box height={1} />
        <input
          value={apiKey()}
          placeholder="sk-..."
          onChange={(value: string) => setApiKey(value)}
          onSubmit={handleSubmit}
        />
        <box height={1} />
        <text fg="#808080">
          Keys are stored in ~/.piko/auth.json and never sent to piko servers.
        </text>
      </box>
    </OverlayContainer>
  );
}
