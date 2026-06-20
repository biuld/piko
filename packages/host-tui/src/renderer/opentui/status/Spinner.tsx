// ============================================================================
// Spinner — simple animated spinner using brailler characters.
// Uses SolidJS onMount + setInterval for frame cycling.
// ============================================================================

import { createSignal, onCleanup, onMount } from "solid-js";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const INTERVAL_MS = 80;

export function Spinner() {
  const [frame, setFrame] = createSignal(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  onMount(() => {
    timer = setInterval(() => {
      setFrame((f) => (f + 1) % FRAMES.length);
    }, INTERVAL_MS);
  });

  onCleanup(() => {
    if (timer) clearInterval(timer);
  });

  // Return frame character with trailing space so parent text doesn't merge
  return <text>{FRAMES[frame()]} </text>;
}
