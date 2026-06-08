// ============================================================================
// StatusText — simple single-line status / loading / empty-state message.
//
// Pure presentational: no state, no keyboard handling.
// ============================================================================

export interface StatusTextProps {
  text: string;
}

export function StatusText(props: StatusTextProps) {
  return (
    <box padding={1}>
      <text>{props.text}</text>
    </box>
  );
}
