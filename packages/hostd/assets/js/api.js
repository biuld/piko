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

// Subscribes to live records for one run. `onRecord` is called for every
// pushed record; the returned function closes the stream. The server keeps
// no-recorder sessions open with keep-alive pings, so there are no reload
// reconnect loops.
export function openRunStream(sessionId, runId, onRecord) {
  const es = new EventSource(
    `/api/trajectory/runs/${encodeURIComponent(runId)}/stream?session_id=${encodeURIComponent(sessionId)}`
  );
  es.onmessage = () => onRecord();
  return () => es.close();
}
