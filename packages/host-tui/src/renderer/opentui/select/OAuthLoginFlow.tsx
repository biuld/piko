import { createSignal, onCleanup, onMount } from "solid-js";
import type { ActionService } from "../action-service.js";
import { useTheme } from "../theme-context.js";
import type { TuiEvent } from "../../../state/events.js";

export interface OAuthLoginFlowProps {
  provider: string;
  providerName: string;
  actionSvc: ActionService;
  surfaceId: string;
  onComplete: (success: boolean, message?: string) => void;
}

export function OAuthLoginFlow(props: OAuthLoginFlowProps) {
  const { provider, providerName, actionSvc, onComplete } = props;
  const theme = useTheme();

  const [status, setStatus] = createSignal<
    | { phase: "init" | "progress"; message: string }
    | { phase: "device_code"; userCode: string; verificationUri: string }
    | { phase: "error"; message: string }
  >({ phase: "init", message: "Starting login..." });

  onMount(() => {
    // Send IPC command to hostd
    actionSvc.host.executeCommand({
      type: "auth_login_start",
      provider,
      command_id: `auth_${Date.now()}`,
    } as any);

    // Subscribe to events
    const unsub = actionSvc.events.subscribe((e: TuiEvent) => {
      if (e.type === "auth_login_device_code" && e.provider === provider) {
        setStatus({
          phase: "device_code",
          userCode: e.user_code,
          verificationUri: e.verification_uri,
        });
      } else if (e.type === "auth_login_success" && e.provider === provider) {
        onComplete(true, `Logged in to ${providerName}`);
      } else if (e.type === "auth_login_failed" && e.provider === provider) {
        const msg = e.error;
        setStatus({ phase: "error", message: msg });
        onComplete(false, msg);
      }
    });

    onCleanup(unsub);
  });

  return (
    <box flexDirection="column" gap={1}>
      <text color={theme.colors.brand}>Login to {providerName}</text>
      {status().phase === "init" || status().phase === "progress" ? (
        <text color={theme.colors.textMuted}>{status().message}</text>
      ) : status().phase === "device_code" ? (
        <box flexDirection="column" gap={1}>
          <text>
            Please go to:{" "}
            <text color={theme.colors.accent}>{(status() as any).verificationUri}</text>
          </text>
          <text>
            And enter the code:{" "}
            <text color={theme.colors.accent} bold>
              {(status() as any).userCode}
            </text>
          </text>
          <text color={theme.colors.textMuted}>Waiting for authorization...</text>
        </box>
      ) : (
        <text color={theme.colors.error}>{(status() as any).message}</text>
      )}
    </box>
  );
}
