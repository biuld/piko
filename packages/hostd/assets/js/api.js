// Hostd trajectory HTTP + SSE client.

async function getJson(path) {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function loadSessions() {
  return getJson("/api/trajectory/sessions");
}

export async function loadRuns(sessionId) {
  const runs = [];
  let cursor = null;
  do {
    const url = `/api/trajectory/runs?session_id=${encodeURIComponent(sessionId)}` +
      (cursor ? `&cursor=${encodeURIComponent(cursor)}` : "") + "&limit=100";
    const page = await getJson(url);
    runs.push(...page.runs);
    cursor = page.nextCursor ?? null;
  } while (cursor);
  return runs;
}

export async function fetchRun(sessionId, runId) {
  return getJson(`/api/trajectory/runs/${encodeURIComponent(runId)}?session_id=${encodeURIComponent(sessionId)}`);
}

// Subscribes to live events for one run. `onRecord` fires for every record
// pushed for the watched run; `onRunsChanged` fires when the session's run
// list changes (a run started/finished — even a different run). The returned
// function closes the stream. The server keeps no-recorder sessions open with
// keep-alive pings, so there are no reload reconnect loops.
export function openRunStream(sessionId, runId, onRecord, onRunsChanged) {
  const es = new EventSource(
    `/api/trajectory/runs/${encodeURIComponent(runId)}/stream?session_id=${encodeURIComponent(sessionId)}`
  );
  es.onmessage = (ev) => {
    // The server pushes `record` (watched run appended a record) and
    // `runs_changed` (session run list changed). `reload` is a plain string,
    // not JSON — it falls through to the record handler and triggers a full
    // refetch, which is the correct response to a lagged broadcast.
    let kind = "record";
    try {
      kind = JSON.parse(ev.data)?.kind ?? "record";
    } catch {
      // reload or malformed payload: treat as a record-style signal.
    }
    if (kind === "runs_changed") {
      if (onRunsChanged) onRunsChanged();
    } else {
      onRecord();
    }
  };
  return () => es.close();
}
