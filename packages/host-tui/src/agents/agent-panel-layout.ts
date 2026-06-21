export interface AgentPanelColumns {
  marker: number;
  gap: number;
  name: number;
  progress: number;
  detail: number;
  queue: number;
}

export function getAgentPanelColumns(width: number): AgentPanelColumns {
  const available = Math.max(20, width);
  const marker = 1;
  const gap = 1;
  const progress = available < 34 ? 5 : 7;
  const queue = available >= 64 ? 12 : 0;
  const name = Math.min(16, Math.max(8, Math.floor(available * 0.22)));
  const gapCount = queue > 0 ? 5 : 4; // before marker + between each pair
  const detail = Math.max(1, available - marker - gap * gapCount - name - progress - queue);
  return { marker, gap, name, progress, detail, queue };
}
