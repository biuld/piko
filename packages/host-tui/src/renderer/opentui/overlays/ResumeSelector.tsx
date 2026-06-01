// ============================================================================
// Resume Session Selector Overlay
// ============================================================================

import { createSignal, createMemo, onMount } from "solid-js";
import type { PikoHost, SessionMeta } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import { OverlayContainer } from "./OverlayContainer.js";

export interface ResumeSelectorProps {
  store: TuiStore;
  host: PikoHost;
  onClose: () => void;
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { store, host, onClose } = props;
  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [search, setSearch] = createSignal("");
  const [loading, setLoading] = createSignal(true);

  // Load sessions on mount
  onMount(async () => {
    try {
      const all = await host.listSessions({});
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
      const name = session.name ?? session.id.slice(0, 8);
      const date = new Date(session.modified).toLocaleDateString();
      return {
        name,
        description: `${date} — ${session.model} — ${session.messageCount} msgs`,
        value: session as any,
      };
    }),
  );

  function handleSelect(_index: number, option: { value?: any } | null): void {
    if (option?.value) {
      const session = option.value as SessionMeta;
      store.dispatch({
        type: "session_resumed",
        sessionId: session.id,
        transcript: [],
      });
    }
    onClose();
  }

  if (loading()) {
    return (
      <OverlayContainer kind="resume" title="Resume Session" onClose={onClose}>
        <text fg="#808080">Loading sessions...</text>
      </OverlayContainer>
    );
  }

  const items = filtered();

  return (
    <OverlayContainer kind="resume" title="Resume Session" onClose={onClose}>
      {/* Search input */}
      <box height={1} paddingBottom={1}>
        <text fg="#808080">Search: </text>
        <input
          value={search()}
          placeholder="Filter sessions..."
          onChange={(value: string) => setSearch(value)}
        />
      </box>

      {/* Session list */}
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
