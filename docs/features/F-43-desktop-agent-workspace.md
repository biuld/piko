# F-43: Desktop agent workspace

> Status: partial (PRs 2–5 landed: tabs, quiet chrome, composer meter, per-tab follow; canvas/picker visuals closed by F-44)
> Priority: P0
> Source evidence: piko product direction after F-42 visual review (screenshot 2026-08-22 21:22)
> Design: [D-60](../design/D-60-desktop-agent-workspace.md)
> Superseded in part by: F-47 composer-attachments (model/thinking pickers move into the composer header; composer becomes two-tone with attach chips)
> Supersedes: F-42 bullets that make the sidebar the agent switcher and the Composer the model/thinking chrome (table below). Unlisted F-42 rules stand.

## Summary

The desktop primary window keeps two columns. The left column discovers and opens sessions. The right column is the **agent workspace** for the open session: a tab for each agent instance, the selected agent’s conversation, and a composer that always targets that tab. Window identity chrome does not compete with the workspace toolbar. Connection, running work, unread activity, and pending human action are visible without duplicating the same words in two places.

## Problem

Users work with one or more agents inside a session. Putting agent selection in the session list and the conversation in a nameless pane forces a split mental model. Mixing transport status, session inventory, and model controls into the window title and the input well hides the conversation and makes live/idle hard to trust.

## User journeys

1. The user opens a live session that has a single root agent. The content island shows **one tab** (stable chrome), that agent’s Timeline (empty or populated), and a composer ready to send to it. The sidebar lists sessions, not a duplicate agent list.
2. A parent spawns child agents. New tabs appear in host order. The user clicks **Researcher**. The Researcher tab highlights immediately, the body is Loading, Send/Cancel are disabled, and the composer shows Researcher’s draft (empty on first visit). After the host selects Researcher, Send is enabled. Main’s draft is restored when Main is selected again.
3. The user is reading older Researcher output while Main is running. Researcher’s tab stays on the scrolled position; Main’s tab shows a running mark. Switching to Main follows the tail if that tab was following.
4. The window is narrowed. The sidebar collapses to a temporary layer. The tab strip remains on the content island. The composer never sits under the sidebar.
5. The user changes model or thinking from the **workspace toolbar**, not from a muted chip in the input. Context fill is a meter on the composer card’s bottom-left.
6. There is no open session. The content island is an empty state with a path to open or create a session. **No fake tabs.**
7. Transport drops. The window chrome shows a single disconnected indicator. Drafts remain. Host-required actions disable. Tabs stay visible but clicks do not select and are not queued. Reconnect hydrates from the host before anything is labeled current.

## In scope

- Right-column tab group whose members are the current session’s agent instances.
- Tab selection as the product action that changes the viewed/submitted agent.
- Sidebar without a primary Agents section.
- Window chrome vs workspace toolbar split.
- Connection, per-agent work, unread, and pending-action presentation.
- Composer visual redesign and context meter, still floating in the Timeline column.
- Return-to-latest rules for empty vs non-empty Timelines.
- Per-tab presentation state (draft, follow, composer error).
- Keyboard and pointer access to tabs, toolbar, timeline, composer, sidebar.
- Loading, empty, error, disconnected, streaming, and restored states for this workspace.

## Out of scope

- Closing or destroying agents from the tab strip.
- A third column, inspector, or terminal/editor workspace.
- Nested tab trees, tab drag-reorder, split-pane per agent.
- Changing host selection semantics, subscription, or Timeline reduction.
- Durable draft persistence across app restart (F-42 resolved question 3 still holds).
- User-resizable sidebar (F-42 resolved question 1 still holds).

## Relationship to F-42

F-43 **supersedes** the F-42 bullets in the table below. They must not remain normative once F-43 lands: the first behavior PR patches F-42 journeys/acceptance, D-59 sidebar/composer placement, and V-59 so the repo cannot ship a desktop that still claims “sidebar selects agents.” Unlisted F-42 rules stand (two columns, Composer-in-Timeline, no third column, host authority, narrow-window overlay).

| F-42 statement | F-43 replacement (supersedes) |
|---|---|
| Sidebar provides session discovery **and the selected session’s agent hierarchy** | Sidebar provides session discovery, New Session, and Settings. Agent switching is the content-island tab strip. |
| Journey 1: sidebar presents session **and agent** navigation | Journey 1: sidebar presents sessions; the island presents agent tabs. |
| Journey 2: user selects another session **or agent in the sidebar** | Session selection remains in the sidebar. Agent selection is a tab action. |
| Acceptance: sidebar can select sessions **and agents** | Sidebar selects sessions (keyboard + pointer). Tabs select agents (keyboard + pointer). Both update the host projection. |
| Composer chrome may expose target agent, model, thinking, context | Target agent is the active tab. Model and thinking live on the workspace toolbar. Context is a composer-adjacent meter. |
| Window controls, navigation, and Timeline actions occupy header zones without two competing title bars | Window chrome is identity + transport + sidebar toggle. Timeline/agent actions occupy the **content island header**, not a second native title bar. |

## Behavior and states

### Spatial model

At a comfortable width:

```
┌ Sidebar (sessions) ┐ ┌ Content island ─────────────────────────────────┐
│ session list       │ │ [ Main | Researcher | Reviewer | ⋯ ]  toolbar…  │
│ New Session        │ │ Timeline for selected agent                     │
│ Settings           │ │                                                 │
│                    │ │              [ Composer for this agent ]        │
└────────────────────┘ └─────────────────────────────────────────────────┘
```

- Two columns only. Timeline (tab body) still receives remaining width.
- Tab strip is the **principal of the content island**, not the native window title bar. One window title bar; one document toolbar. They must not look like two competing title bars (F-42).
- Composer floats inside the active tab body, bounded width, never under the sidebar.

### Agent tabs

- Tab membership **is** the current session’s agent list from the host projection. The client does not invent, hide, or synthesize agents.
- **Order (v1):** host agent-list order (parents before children). The desktop does not re-sort.
- **Label:** agent name if non-empty, else agent id. Tooltip always includes `{label} · {agent instance id}` (full id). If two visible tabs would share a label, the visible title becomes `{label} · {last 8 characters of instance id}`.
- **Single agent:** still show one tab so the workspace chrome is stable.
- **When tabs exist:** if and only if the live session’s agent list is non-empty, including after disconnect while that projection is still in memory. No session, or an empty agent list → **no tabs**. Do not fabricate a Main tab.
- **Selection:** the highlighted tab is the **view target** — an in-flight select-agent if any, otherwise the host-selected agent. A click or keyboard activate no-ops if it is already the view target. Any other tab issues select-agent. While a select is in flight, the body is Loading and **Send/Cancel are disabled** so they cannot hit the previous host agent.
- **Disconnected:** last projected tabs stay visible but are **non-activating**. Clicks and keyboard activate no-op; nothing is queued for reconnect.
- **Close:** not offered. Agents appear and disappear when the host list changes.
- **Overflow:** when tabs do not fit, a contiguous visible range that includes the selected tab remains on the strip; the rest go to a **More** menu.
- **Hierarchy:** not indented in v1. Child-ness is available as a tooltip rather than a nested tab tree.

### Tab status marks

Each tab may show compact marks for **that** agent only. One mark per tab.

| Priority | Condition | Visible mark |
|---|---|---|
| 1 | Requires operator action | Attention |
| 2 | Running, queued, or cancelling | Busy dot |
| 3 | Unread reports and the tab is not the view target | Unread count |
| — | Closed / terminated / unavailable lifecycle | Quiet muted label; still selectable if the host still lists the agent |

Idle + no unread = no badge. Session-level “Needs attention · N” in the window title bar is **removed**. Discovery of other agents’ pending work is the tab mark.

### Window chrome

- Principal: product identity (`piko`).
- Trailing: **one** connection indicator + sidebar show/hide.
- Connection values remain connecting / hydrating / live / disconnected / decode-error, shown once. Never `live` beside `Live`.
- Session count is **not** window chrome. The sidebar list is the inventory.
- Model, thinking, context, and per-agent attention are **not** window chrome.

### Workspace toolbar (content island header)

Associated with the active tab:

- Model — labeled chrome action, not a muted chip. Tooltip: **“Session model (next turn)”** plus the model id. Opens an **anchored menu** (F-44); not a dialog overlay.
- Thinking — same treatment. Tooltip: **“Session thinking level (next turn)”**. Opens an **anchored menu** (F-44).
- If the **view-target** agent requires action, a Needs attention control opens the existing attention **dialog**. Other agents’ attention is tab-only.

These controls remain session-scoped. Do not label them “Main’s model.” When there is no live session, the island header is omitted or inert.

### Timeline (tab body)

- Canonical selected-agent projection; stable item identity.
- Session or agent change enters loading/empty **before** new items; previous target’s rows are never labeled current.
- Follow-tail vs reading is **per tab**.
- Return-to-latest appears only when the Timeline is ready with at least one row and the user is not at the tail. Never on empty, loading, error, or no-session.

### Composer

Anatomy (structure, not pixels):

```text
┌ Composer card (one outer radius; Timeline column, bounded width) ─────┐
│ Send failed: …                                          (error only)  │
│ Message {view-target label}…                                          │
│ [meter] 12k/128k                          [Cancel if running] [Send]  │
└───────────────────────────────────────────────────────────────────────┘
```

F-44 **supersedes** the nested input-well diagram. The Composer is a single elevated island; the textarea is flush (no inner well).

- Still floats in the active tab’s Timeline column with bottom/side separation (F-42).
- Placeholder names the view-target agent. Drafts swap when the view target changes and are kept per agent in memory only.
- Send and Cancel are disabled while a select-agent is in flight, and when the session is not live.
- Empty submit is a no-op. Failed submit keeps the draft and shows an error on that tab. Accepted submit clears only the submitted draft if it was not edited further.
- Model/thinking are **removed** from the composer row.
- Context fill is the **bottom-left of the same card**: a compact meter plus used/window text when both known; omit the meter when unknown — no “Context —” chip.
- Cancel is shown only when the view-target agent is running.

### Sidebar

- Sections: session list; Application (New Session, Settings). **No Agents section.**
- Selecting a session still opens/hydrates it. The host-selected agent becomes the selected tab after hydrate.
- Narrow-window collapse to a temporary layer is unchanged (F-42).

### Focus and keyboard

Primary surfaces: Sidebar, Agent tabs, Timeline, Composer.

- Keyboard traversal actually focuses the tab strip.
- When the tab strip holds focus: Left/Right/Home/End move among tabs and activate on move (same as clicking).
- Pointer down on Timeline, Composer, or sidebar blurs the tab strip so arrows do not keep switching agents.
- Escape dismisses the top overlay or narrow sidebar; it never discards a draft and never closes a tab.

### Loading, empty, error, and disconnected

| Situation | Tabs | Body | Composer | Window chrome |
|---|---|---|---|---|
| Connecting / hydrating, no live session yet | None | Loading or no-session empty | Disabled | Single connecting/hydrating indicator |
| No session selected | None | “No session selected” + path to sidebar/New Session | Hidden or disabled | Live or current transport |
| Opening a session | None until agents exist | Loading | Disabled | Hydrating if bootstrap, else live |
| Live, agents present, Timeline empty | Tabs shown; one selected | Conversation empty; composer ready | Enabled if live | Quiet live |
| Switching agent | Highlight in-flight target | Loading; no stale rows | Draft swaps immediately; Send/Cancel disabled | Unchanged |
| Select-agent failed | Highlight remains last host-selected | Error for the failed switch | View target rolls back; drafts intact | Unchanged |
| Agent Timeline ready | Selected tab matches host | Rows | Enabled | Quiet live |
| Streaming, following | Busy mark on that tab | Tail pinned | Cancel if running | Quiet live |
| Streaming, reading | Busy mark | Position held; return-to-latest if non-empty | As live | Quiet live |
| Other agent needs approval | Attention mark on **that** tab | Unchanged | Unchanged | No global attention chip |
| Disconnected / decode-error | Last tabs visible, non-activating | Last content or error; not labeled live | Host actions disabled; draft kept | Single disconnected / decode-error indicator |
| Restart | Restored only after host reconcile | Same | In-memory drafts lost (F-42) | Prefs restore window/sidebar only |

## Acceptance criteria

- [ ] At a comfortable width the window shows a session sidebar and a content island whose header is an agent tab strip, with no permanent third column and no Agents section in the sidebar.
- [ ] Tabs are exactly the live session’s host agent instances; a session with one agent shows one tab; no session shows zero tabs.
- [ ] Activating a tab issues the host select-agent action; the Timeline never presents the previous agent’s rows as the new agent; loading/failure states are distinct.
- [ ] Composer drafts, follow-versus-reading, return-to-latest visibility, and composer errors are independent per agent in the session.
- [ ] Return-to-latest never appears on empty, loading, error, or no-session bodies.
- [ ] Window chrome shows product identity, one connection state, and sidebar toggle — not session count, not duplicated Live, not model/thinking.
- [ ] Model and thinking are workspace-toolbar actions; context fill is a composer-adjacent meter when known.
- [ ] Composer remains inside the Timeline column and never under the sidebar; Submit/Cancel are explicit; empty submit is a no-op; failed submit keeps the draft on that tab.
- [ ] Tab overflow keeps the selected tab visible and lists hidden agents in More.
- [ ] Narrow-window sidebar overlay still works; tab strip stays on the island.
- [ ] Disconnect/decode-error cannot look live; drafts survive; reconnect reconciles from the host.
- [ ] Keyboard can move among sidebar, tabs, timeline, and composer without a pointer.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where do agents live? | Content-island tabs | Conversation and target belong together; sidebar stays session inventory |
| Hide tab strip for a single agent? | No; always one tab | Stable workspace chrome; avoids a jump when a child spawns |
| Fake tabs with no session? | No | Would fabricate product state |
| Child agents | Flat tabs in host order; overflow to More | Host list is already parent-before-child; nested tabs add chrome without new actions |
| Tab close | Out of scope | No dismiss-agent client intent; agents are host-lifecycle |
| Model/thinking placement | Workspace toolbar | Session-level intents, but they belong with the turn being composed, not in the input well |
| Context placement | Composer-adjacent meter | Fill is about the next prompt, not window identity |
| Connection presentation | One quiet indicator | F-42 already requires observable transport; duplication is a bug |
| Session count in chrome | Removed | Sidebar is the inventory |
| Attention in window chrome | Removed; tab + active-toolbar | Per-agent fact must sit on the agent |
| Inspector column | Still rejected | F-42 |
| Authority | Host projections only | ADR-022 / D-34 |

## Fusion decisions (codex-rs)

Not derived from codex-rs. N/A.

## Open questions

None blocking v1. Defaults are in Product decisions.

Deferred (not this feature):

- Nested tabs if child count becomes a real findability problem.
- Tab close if a host “dismiss/hide agent from workspace” intent is specified later.
- Persistent drafts across app restart (F-42 Q3).
- Per-agent context windows if later exposed on the agent projection.

## Reference evidence

- Screenshot 2026-08-22 21:22: empty island, sidebar Agents, duplicated Live, return-to-latest on empty, muted composer chips.
- [F-42 Desktop GUI shell](F-42-desktop-gui-shell.md)
- [D-59](../design/D-59-desktop-gui-shell.md), [D-60](../design/D-60-desktop-agent-workspace.md)
- [ADR-022](../decisions/ADR-022-desktop-client-reintroduction.md)
- [F-10](F-10-multi-agent.md), [F-22](F-22-client-agent-projection.md)
