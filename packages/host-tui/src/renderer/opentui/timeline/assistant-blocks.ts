import type { TimelineItem } from "../../../timeline/types.js";

export function getVisibleAssistantBlocks(item: TimelineItem) {
  return (item.content ?? []).filter((block) => block.type === "text" || block.type === "thinking");
}
