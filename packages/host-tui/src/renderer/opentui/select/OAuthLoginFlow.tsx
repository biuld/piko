import { createSignal, onMount } from "solid-js";
import type { ActionService } from "../action-service.js";
import { useTheme } from "../theme-context.js";

export interface OAuthLoginFlowProps {
  provider: string;
  providerName: string;
  actionSvc: ActionService;
  surfaceId: string;
  onComplete: (success: boolean, message?: string) => void;
}

type OAuthStatus =
  | { phase: "init" | "progress"; message: string }
  | { phase: "device_code"; userCode: string; verificationUri: string }
  | { phase: "error"; message: string };

export function OAuthLoginFlow(props: OAuthLoginFlowProps) {
  const { provider, providerName, actionSvc, onComplete } = props;
  const theme = useTheme();

  const [status, setStatus] = createSignal<OAuthStatus>({
    phase: "init",
    message: "Starting login...",
  });

  onMount(() => {
    try {
      actionSvc.startAuthLogin(provider);
      setStatus({
        phase: "progress",
        message: "Waiting for hostd login instructions...",
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus({ phase: "error", message });
      onComplete(false, message);
    }
  });

  const currentStatus = status();

  return (
    <box flexDirection="column" gap={1}>
      <text fg={theme.color("text.accent")}>Login to {providerName}</text>

      {currentStatus.phase === "init" || currentStatus.phase === "progress" ? (
        <text fg={theme.color("text.muted")}>{currentStatus.message}</text>
      ) : currentStatus.phase === "device_code" ? (
        <box flexDirection="column" gap={1}>
          <text>Please go to: {currentStatus.verificationUri}</text>
          <text>And enter the code: {currentStatus.userCode}</text>
          <text fg={theme.color("text.muted")}>Waiting for authorization...</text>
        </box>
      ) : (
        <text fg={theme.color("text.error")}>{currentStatus.message}</text>
      )}
    </box>
  );
}
