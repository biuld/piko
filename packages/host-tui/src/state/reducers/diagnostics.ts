// ============================================================================
// Diagnostics helper — shared sequence validation with logging
// ============================================================================

import type { TimelineProjection } from "../../timeline/projection.js";
import { validateAndApplySeq } from "../../timeline/projection.js";

export function applySeq(
  proj: TimelineProjection,
  runId: string,
  eventSeq: number | undefined,
): TimelineProjection {
  if (eventSeq === undefined) return proj;
  const result = validateAndApplySeq(proj, runId, eventSeq);
  for (const d of result.diagnostics) {
    console.warn(
      `[timeline] ${d.kind}`,
      `runId=${(d as any).runId}`,
      `eventSeq=${(d as any).eventSeq}`,
      `prevSeq=${(d as any).prevSeq}`,
    );
  }
  return result.proj;
}
