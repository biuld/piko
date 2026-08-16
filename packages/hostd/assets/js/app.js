// Composition root: wires the store, the API client, and the views.

import { $, SESSION_KEY, RUN_KEY, short } from "./format.js";
import { loadSessions, loadRuns, fetchRun, openRunStream } from "./api.js";
import { createStore } from "./store.js";
import { renderSessions, renderRunStrip, renderRunStats } from "./panels.js";
import { createMessages, deriveMessageItems } from "./messages.js";
import { createTimeline, deriveTimeline } from "./timeline.js";

const store = createStore({
  sessions: [],
  runs: [],
  selectedSession: localStorage.getItem(SESSION_KEY) || "",
  selectedRun: localStorage.getItem(RUN_KEY) || "",
  loading: false,
  run: null,
  messages: [],
  timelineItems: [],
  tracks: [],
  trackItems: [],
  selectedMessage: -1,
});

const messages = createMessages({
  onSelect: (index) => store.set({ selectedMessage: index }, "message:selected"),
});
const timeline = createTimeline({
  onSelectMessage: (index) => store.set({ selectedMessage: index }, "message:selected"),
});

let streamCleanup = null;
let refreshing = false;

function applyLoading(state) {
  $("loading").classList.toggle("hidden", !state.loading);
}

const actions = {
  setStatus(text) {
    $("status").textContent = text;
  },

  async selectSession(sessionId) {
    store.set({ selectedSession: sessionId, loading: true }, "session:selected");
    localStorage.setItem(SESSION_KEY, sessionId);
    actions.setStatus("loading runs…");
    try {
      const runs = await loadRuns(sessionId);
      store.set({ runs }, "runs:loaded");
      const remembered = runs.find((r) => r.runId === store.state.selectedRun);
      if (remembered) {
        await actions.selectRun(remembered.runId, sessionId);
      } else if (runs.length) {
        await actions.selectRun(runs[0].runId, sessionId);
      } else {
        store.set(
          { run: null, messages: [], timelineItems: [], tracks: [], trackItems: [], selectedMessage: -1 },
          "run:selected"
        );
        actions.setStatus(`${runs.length} runs`);
      }
    } catch (error) {
      actions.setStatus(`runs: ${error.message}`);
    } finally {
      store.set({ loading: false }, "loading:done");
    }
  },

  async selectRun(runId, sessionId = store.state.selectedSession) {
    store.set({ selectedRun: runId, loading: true }, "run:selecting");
    localStorage.setItem(RUN_KEY, runId);
    try {
      const run = await fetchRun(sessionId, runId);
      const messageItems = deriveMessageItems(run);
      const derived = deriveTimeline(run, messageItems);
      store.set(
        { run, messages: messageItems, selectedMessage: -1, ...derived },
        "run:selected"
      );
      actions.setStatus(`run ${short(runId)} · ${run.records?.length ?? 0} records`);
      if (streamCleanup) {
        streamCleanup();
        streamCleanup = null;
      }
      streamCleanup = openRunStream(sessionId, runId, () => {
        if (store.state.selectedRun === runId) actions.refreshRun(sessionId, runId);
      });
    } catch (error) {
      actions.setStatus(`run: ${error.message}`);
    } finally {
      store.set({ loading: false }, "loading:done");
    }
  },

  // Idempotent live refresh: never rebuilds the tree; views append/redraw.
  async refreshRun(sessionId = store.state.selectedSession, runId = store.state.selectedRun) {
    if (!runId || refreshing) return;
    refreshing = true;
    try {
      const run = await fetchRun(sessionId, runId);
      const messageItems = deriveMessageItems(run);
      const derived = deriveTimeline(run, messageItems);
      store.set({ run, messages: messageItems, ...derived }, "run:refreshed");
    } catch (error) {
      // Transient; the stream keeps trying and will retry on the next record.
    } finally {
      refreshing = false;
    }
  },
};

store.subscribe((state, action) => {
  applyLoading(state);
  switch (action) {
    case "sessions:loaded":
    case "session:selected":
      renderSessions(state, actions);
      break;
    case "runs:loaded":
    case "run:selecting":
      renderRunStrip(state, actions);
      break;
    case "run:selected":
      messages.render(state);
      timeline.render(state);
      renderRunStats(state);
      break;
    case "run:refreshed":
      messages.append(state);
      timeline.update(state);
      renderRunStats(state);
      break;
    case "message:selected":
      timeline.update(state);
      messages.highlight(state.selectedMessage);
      break;
  }
});

$("refresh").onclick = () => {
  actions.selectSession(store.state.selectedSession).catch(() => {});
};

async function boot() {
  try {
    const sessions = await loadSessions();
    store.set({ sessions }, "sessions:loaded");
    if (sessions.includes(store.state.selectedSession)) {
      await actions.selectSession(store.state.selectedSession);
    } else if (sessions.length) {
      await actions.selectSession(sessions[0]);
    } else {
      actions.setStatus("no sessions — open/resume one in piko first");
    }
  } catch (error) {
    actions.setStatus(`sessions: ${error.message}`);
  }
}

boot();
