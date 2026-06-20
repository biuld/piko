// ============================================================================
// Spinner — simple animated spinner using brailler characters.
// Uses SolidJS onMount + setInterval for frame cycling.
// ============================================================================

import { createSignal, onCleanup, onMount } from "solid-js";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const INTERVAL_MS = 80;

export interface SpinnerProps {
  /** Lets a containing panel drive one shared animation clock. */
  frame?: number;
  trailingSpace?: boolean;
  fg?: string;
}

export function Spinner(props: SpinnerProps = {}) {
  const [frame, setFrame] = createSignal(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  onMount(() => {
    if (props.frame !== undefined) return;
    timer = setInterval(() => {
      setFrame((f) => (f + 1) % FRAMES.length);
    }, INTERVAL_MS);
  });

  onCleanup(() => {
    if (timer) clearInterval(timer);
  });

  // Return frame character with trailing space so parent text doesn't merge
  const currentFrame = () => props.frame ?? frame();
  return (
    <text fg={props.fg}>
      {FRAMES[currentFrame() % FRAMES.length]}
      {props.trailingSpace === false ? "" : " "}
    </text>
  );
}
