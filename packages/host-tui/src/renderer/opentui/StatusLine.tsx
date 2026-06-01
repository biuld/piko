// ============================================================================
// StatusLine — shows streaming progress / tool status / queue info
// ============================================================================

export interface StatusLineProps {
  entries: string[];
  visible: boolean;
}

export function StatusLine(props: StatusLineProps) {
  const { entries, visible } = props;

  if (!visible) return null;

  return (
    <box flexShrink={0} height={1} paddingLeft={1} paddingRight={1}>
      <text fg="#808080">{entries.join(" │ ")}</text>
    </box>
  );
}
