import type { SelectItem } from "@earendil-works/pi-tui";
import type { SessionMeta } from "piko-host-runtime";

export function formatRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function sortSessions(items: SessionMeta[]): SessionMeta[] {
  return [...items].sort((a, b) => b.modified.localeCompare(a.modified));
}

function buildSessionChildrenMap(sessions: SessionMeta[]): Map<string | null, SessionMeta[]> {
  const byPath = new Map(sessions.map((session) => [session.path, session]));
  const children = new Map<string | null, SessionMeta[]>();

  for (const session of sessions) {
    const parentPath =
      session.parentSessionPath && byPath.has(session.parentSessionPath)
        ? session.parentSessionPath
        : null;
    const siblings = children.get(parentPath) ?? [];
    siblings.push(session);
    children.set(parentPath, siblings);
  }

  return children;
}

export function formatSessionTreeLines(sessions: SessionMeta[]): string[] {
  const children = buildSessionChildrenMap(sessions);
  const lines: string[] = [];

  const walk = (session: SessionMeta, prefix: string, isLast: boolean, depth: number): void => {
    const branch = depth > 0 ? `${prefix}${isLast ? "└─ " : "├─ "}` : "";
    const label = session.name ?? session.id.slice(-8);
    lines.push(
      `${branch}${label}  ${session.model}  ${session.messageCount}msgs  ${formatRelativeTime(session.modified)}`,
    );
    const nextPrefix = `${prefix}${depth > 0 ? (isLast ? "   " : "│  ") : ""}`;
    const descendants = sortSessions(children.get(session.path) ?? []);
    descendants.forEach((child, index) => {
      walk(child, nextPrefix, index === descendants.length - 1, depth + 1);
    });
  };

  const roots = sortSessions(children.get(null) ?? []);
  roots.forEach((root, index) => {
    walk(root, "", index === roots.length - 1, 0);
  });
  return lines;
}

export function createThreadedSessionSelectItems(sessions: SessionMeta[]): SelectItem[] {
  const children = buildSessionChildrenMap(sessions);
  const items: SelectItem[] = [];

  const walk = (session: SessionMeta, prefix: string, isLast: boolean, depth: number): void => {
    const branch = depth > 0 ? `${prefix}${isLast ? "└─ " : "├─ "}` : "";
    const label = `${branch}${session.name ?? session.id.slice(-8)}`;
    items.push({
      value: session.path,
      label,
      description: `${session.model}  ${session.messageCount}msgs  ${formatRelativeTime(session.modified)}  ${session.cwd}`,
    });
    const nextPrefix = `${prefix}${depth > 0 ? (isLast ? "   " : "│  ") : ""}`;
    const descendants = sortSessions(children.get(session.path) ?? []);
    descendants.forEach((child, index) => {
      walk(child, nextPrefix, index === descendants.length - 1, depth + 1);
    });
  };

  const roots = sortSessions(children.get(null) ?? []);
  roots.forEach((root, index) => {
    walk(root, "", index === roots.length - 1, 0);
  });
  return items;
}
