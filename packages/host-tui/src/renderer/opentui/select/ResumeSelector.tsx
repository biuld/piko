// ============================================================================
// Resume Session Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createSignal, createMemo, onCleanup, onMount } from "solid-js";
import type { SessionMeta } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";

export interface ResumeSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

function clamp(n: number, max: number): number {
  return Math.max(0, Math.min(max, n));
}

export function ResumeSelector(props: ResumeSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;
  const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
  const [query, setQuery] = createSignal("");
  const [selectedIdx, setSelectedIdx] = createSignal(0);
  const [loading, setLoading] = createSignal(true);
  const [switching, setSwitching] = createSignal(false);

  // Load sessions and register keyboard handler
  onMount(async () => {
    try {
      const all = await actionSvc.host.listSessions({});
      setSessions(all);
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  });

  const items = createMemo<SelectItem<string>[]>(() => {
    const q = query().toLowerCase().trim();
    const all = sessions();
    const filtered = q
      ? all.filter(
          (s) =>
            (s.name ?? "").toLowerCase().includes(q) ||
            s.id.toLowerCase().includes(q),
        )
      : all;

    return filtered.map((session) => {
      const name = session.name ?? session.id.slice(0, 12);
      const date = new Date(session.modified).toLocaleDateString();
      return {
        id: session.id,
        label: name,
        description: `${date} — ${session.model} — ${session.messageCount} msgs`,
        value: session.path,
      };
    });
  });

  const itemCount = () => items().length;

  async function confirm(): Promise<void> {
    if (switching()) return;
    const idx = clamp(selectedIdx(), itemCount() - 1);
    const item = items()[idx];
    if (!item) return;

    setSwitching(true);
    try {
      await actionSvc.switchSession(item.value);
    } catch {
      // Session switch may fail
    } finally {
      setSwitching(false);
      onClose();
    }
  }

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): boolean {
        if (event.name === "up") {
          setSelectedIdx((i) => clamp(i - 1, itemCount() - 1));
          return true;
        }
        if (event.name === "down") {
          setSelectedIdx((i) => clamp(i + 1, itemCount() - 1));
          return true;
        }
        if (event.name === "enter" || event.name === "return") {
          confirm();
          return true;
        }
        if (event.name === "escape") {
          onClose();
          return true;
        }
        if (event.name === "backspace") {
          setQuery((q) => q.slice(0, -1));
          setSelectedIdx(0);
          return true;
        }
        if (event.char && event.char.length === 1 && event.char >= " ") {
          setQuery((q) => q + event.char);
          setSelectedIdx(0);
          return true;
        }
        return false;
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  if (loading()) {
    return (
      <SelectorShell title="Resume Session" onClose={onClose}>
        <text>Loading sessions...</text>
      </SelectorShell>
    );
  }

  if (switching()) {
    return (
      <SelectorShell title="Resume Session" onClose={onClose}>
        <text>Switching session...</text>
      </SelectorShell>
    );
  }

  return (
    <SelectorShell
      title="Resume Session"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel  Type to filter"]}
    >
      <box height={1} paddingBottom={1}>
        <text>{query() || "Type to filter sessions..."}</text>
      </box>

      <SelectListView
        items={items()}
        selectedIndex={selectedIdx()}
        maxHeight={12}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
