// ============================================================================
// TimelineSeparator — full-width border separator between message groups
// ============================================================================

import { useTheme } from "../theme-context.js";

export function TimelineSeparator() {
  const theme = useTheme();
  return (
    <box
      height={1}
      border={["bottom"]}
      borderColor={theme.color("border.muted")}
      paddingLeft={1}
      paddingRight={1}
    />
  );
}
