export interface AgentPanelColumns {
  marker: number;
  name: number;
  progress: number;
  detail: number;
  queue: number;
}

export function getAgentPanelColumns(width: number): AgentPanelColumns {
  const available = Math.max(20, width);
  const marker = available < 40 ? 2 : 4;
  const progress = available < 34 ? 5 : 7;
  const queue = available >= 64 ? 12 : 0;
  const name = Math.min(16, Math.max(8, Math.floor(available * 0.22)));
  const detail = Math.max(1, available - marker - name - progress - queue);
  return { marker, name, progress, detail, queue };
}
