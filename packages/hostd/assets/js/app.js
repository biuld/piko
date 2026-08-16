// Composition root: wires the store, the API client, and the three views.

import { $, SESSION_KEY, RUN_KEY, short } from "./format.js";
import { loadSessions, loadRuns, fetchRun, openRunStream } from "./api.js";
import { createStore } from "./store.js";
import { renderSessions, renderRunStrip, renderRunStats } from "./panels.js";
import { createMessages } from "./messages.js";
import { createTimeline, deriveTimeline } from "./timeline.js";
import { createPrompt } from "./prompt.js";

const store = createStore({
  sessions: [],
  runs: [],
  selectedSession: localStorage.getItem(SESSION_KEY) || "",
  selectedRun: localStorage.getItem(RUN_KEY) || "",
  activeTab: "conversation",
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
  onSelectPrompt: () => actions.selectTab("prompt"),
});
const prompt = createPrompt();

let streamCleanup = null;
let refreshing = false;

function applyTab(state) {
  const conversationView = $("conversation-view");
  const promptView = $("prompt-view");
  const tabs = $("run-tabs");
  tabs.classList.toggle("hidden", !state.run);
  conversationView.classList.toggle("hidden", state.activeTab !== "conversation");
  promptView.classList.toggle("hidden", state.activeTab !== "prompt");
  for (const button of tabs.querySelectorAll(".tab")) {
    button.classList.toggle("active", button.dataset.tab === state.activeTab);
  }
}

function applyLoading(state) {
  $("loading").classList.toggle("hidden", !state.loading);
}

const actions = {
  setStatus(text) {
    $("status").textContent = text;
  },

  selectTab(tab) {
    store.set({ activeTab: tab }, "tab:selected");
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
      const derived = deriveTimeline(run);
      store.set(
        { run, messages: run.messages || [], selectedMessage: -1, ...derived },
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
      const derived = deriveTimeline(run);
      store.set({ run, messages: run.messages || [], ...derived }, "run:refreshed");
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
      prompt.render(state);
      renderRunStats(state);
      applyTab(state);
      break;
    case "run:refreshed":
      messages.append(state);
      timeline.update(state);
      prompt.update(state);
      renderRunStats(state);
      break;
    case "tab:selected":
      applyTab(state);
      prompt.render(state);
      break;
    case "message:selected":
      timeline.update(state);
      messages.highlight(state.selectedMessage);
      break;
  }
});

for (const button of document.querySelectorAll("#run-tabs .tab")) {
  button.addEventListener("click", () => actions.selectTab(button.dataset.tab));
}

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
