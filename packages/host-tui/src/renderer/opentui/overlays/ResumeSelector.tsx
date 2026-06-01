// ============================================================================
// Resume Session Selector Overlay
// Calls host.switchSession() to actually change the active session
// ============================================================================

import { createSignal, createMemo, onMount } from "solid-js";
import type { SessionMeta } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import { OverlayContainer } from "./OverlayContainer.js";

export interface ResumeSelectorProps {
  actionSvc: ActionService;
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { actionSvc, onClose } = props;
  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [search, setSearch] = createSignal("");
  const [loading, setLoading] = createSignal(true);
  const [switching, setSwitching] = createSignal(false);

  onMount(async () => {
    try {
      const all = await actionSvc.host.listSessions({});
      setSessions(all);
    } catch {
      // Ignore load errors
    } finally {
      setLoading(false);
    }
  });

  const filtered = createMemo(() => {
    const q = search().trim().toLowerCase();
    const all = sessions();
    if (!q) return all;
    return all.filter((s) => {
      const name = (s.name ?? "").toLowerCase();
      const id = s.id.toLowerCase();
      return name.includes(q) || id.includes(q);
    });
  });

  const options = createMemo(() =>
    filtered().map((session) => {
      const name = session.name ?? session.id.slice(0, 12);
      const date = new Date(session.modified).toLocaleDateString();
      return {
        name,
        description: `${date} — ${session.model} — ${session.messageCount} msgs`,
        value: session.path,
      };
    }),
  );

  async function handleSelect(_index: number, option: { value?: string } | null): Promise<void> {
    if (!option?.value || switching()) return;
    const specifier = option.value;

    setSwitching(true);
    try {
      // Actually switch the host session
      await actionSvc.switchSession(specifier);
    } catch {
      // Session switch may fail if session file is invalid
    } finally {
      setSwitching(false);
      onClose();
    }
  }

  if (loading()) {
    return (
      <OverlayContainer kind="resume" title="Resume Session" onClose={onClose}>
        <text fg="#808080">Loading sessions...</text>
      </OverlayContainer>
    );
  }

  if (switching()) {
    return (
      <OverlayContainer kind="resume" title="Resume Session" onClose={onClose}>
        <text fg="#8abeb7">Switching session...</text>
      </OverlayContainer>
    );
  }

  const items = filtered();

  return (
    <OverlayContainer kind="resume" title="Resume Session" onClose={onClose}>
      <box height={1} paddingBottom={1}>
        <text fg="#808080">Search: </text>
        <input
          value={search()}
          placeholder="Filter sessions..."
          onChange={(value: string) => setSearch(value)}
        />
      </box>

      <box flexGrow={1}>
        {items.length > 0 ? (
          <select
            options={options()}
            selectedIndex={0}
            showDescription
            height={Math.min(items.length + 2, 12)}
            onSelect={handleSelect}
          />
        ) : (
          <text fg="#808080">No sessions found</text>
        )}
      </box>
    </OverlayContainer>
  );
}
