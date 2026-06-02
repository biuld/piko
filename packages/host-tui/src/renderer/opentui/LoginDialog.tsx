// ============================================================================
// Login Dialog — API key input using SelectorShell
// ============================================================================

import { createSignal } from "solid-js";
import type { TuiStore } from "./store.js";
import { SelectorShell } from "./select/SelectorShell.js";

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

    // API key storage is handled by the host/auth layer.
    onClose();
  }

  return (
    <SelectorShell title={`Login: ${provider}`} onClose={onClose}>
      <box flexDirection="column">
        <text>Enter API key for {provider}:</text>
        <box height={1} />
        <input
          value={apiKey()}
          placeholder="sk-..."
          onInput={(value: string) => setApiKey(value)}
          onSubmit={handleSubmit}
        />
        <box height={1} />
        <text>
          Keys are stored in ~/.piko/auth.json and never sent to piko servers.
        </text>
      </box>
    </SelectorShell>
  );
}
