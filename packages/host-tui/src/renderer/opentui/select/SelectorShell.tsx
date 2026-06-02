// ============================================================================
// SelectorShell — reusable selector wrapper with title, content, hints
// ============================================================================

import type { JSX } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface SelectorShellProps {
  title: string;
  children: JSX.Element;
  hints?: string[];
  onClose: () => void;
  compact?: boolean;
}

export function SelectorShell(props: SelectorShellProps) {
  const theme = useTheme();
  const { title, children, hints, onClose, compact = false } = props;

  return (
    <box
      border
      borderColor={theme.color("border.accent")}
      flexDirection="column"
      padding={1}
    >
      {/* Title row */}
      <box flexDirection="row" justifyContent="space-between" height={1}>
        <text fg={theme.color("text.accent")}>
          <strong>{title}</strong>
        </text>
        {!compact && (
          <text fg={theme.color("text.dim")}>Esc to close</text>
        )}
      </box>

      {/* Content */}
      <box flexDirection="column" paddingTop={1} paddingBottom={1}>
        {children}
      </box>

      {/* Hint row */}
      {hints && hints.length > 0 && (
        <box height={1}>
          <text fg={theme.color("text.dim")}>{hints.join("  ")}</text>
        </box>
      )}
    </box>
  );
}
