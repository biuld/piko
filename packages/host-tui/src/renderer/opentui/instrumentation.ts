// ============================================================================
// Dev-only renderer instrumentation — gated by PIKO_TUI_TRACE env var.
// Traces global dispatch count, render plan changes, surface open/close,
// timeline item count, and scrollbox viewport changes.
// ============================================================================

const TRACE_ENABLED = process.env.PIKO_TUI_TRACE === "1";

// ============================================================================
// Counters
// ============================================================================

let dispatchCount = 0;
let lastDispatchTime = 0;
let dispatchRate = 0;

let renderCount = 0;
let lastRenderTime = 0;
let renderRate = 0;

// ============================================================================
// Public API
// ============================================================================

export function traceDispatch(eventType: string): void {
  if (!TRACE_ENABLED) return;

  dispatchCount++;
  const now = Date.now();
  if (now - lastDispatchTime >= 1000) {
    dispatchRate = dispatchCount;
    dispatchCount = 0;
    lastDispatchTime = now;
    if (dispatchRate > 30) {
      console.error(`[tui trace] ⚠️ high dispatch rate: ${dispatchRate}/s (type: ${eventType})`);
    }
  }

  console.error(`[tui trace] dispatch: ${eventType} (rate: ${dispatchRate}/s)`);
}

export function traceRender(stateSnapshot: {
  timelineItemCount: number;
  surfaceCount: number;
  viewportWidth: number;
  viewportHeight: number;
}): void {
  if (!TRACE_ENABLED) return;

  renderCount++;
  const now = Date.now();
  if (now - lastRenderTime >= 1000) {
    renderRate = renderCount;
    renderCount = 0;
    lastRenderTime = now;
  }

  console.error(
    `[tui trace] render: items=${stateSnapshot.timelineItemCount} ` +
      `surfaces=${stateSnapshot.surfaceCount} ` +
      `viewport=${stateSnapshot.viewportWidth}x${stateSnapshot.viewportHeight} ` +
      `(rate: ${renderRate}/s)`,
  );
}

export function traceSurfaceOpen(surfaceId: string, role: string, mount: string): void {
  if (!TRACE_ENABLED) return;
  console.error(`[tui trace] surface opened: ${surfaceId} role=${role} mount=${mount}`);
}

export function traceSurfaceClose(surfaceId: string): void {
  if (!TRACE_ENABLED) return;
  console.error(`[tui trace] surface closed: ${surfaceId}`);
}

export function traceTimelineResize(oldHeight: number, newHeight: number): void {
  if (!TRACE_ENABLED) return;
  if (oldHeight !== newHeight) {
    console.error(`[tui trace] timeline viewport height changed: ${oldHeight} → ${newHeight}`);
  }
}

let lastTimelineHeight: number | undefined;

export function traceTimelineHeight(height: number): void {
  if (!TRACE_ENABLED) return;
  if (lastTimelineHeight !== undefined && lastTimelineHeight !== height) {
    console.error(
      `[tui trace] ⚠️ timeline scrollbox height changed: ${lastTimelineHeight} → ${height}`,
    );
  }
  lastTimelineHeight = height;
}
