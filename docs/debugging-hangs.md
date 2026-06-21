# Diagnosing hangs

Run piko with structured tracing enabled:

```bash
PIKO_DEBUG=1 ./dist-bundle/piko
```

Trace files are written to `~/.piko/logs/piko-<timestamp>-<pid>.jsonl`. Set
`PIKO_DEBUG_LOG=/absolute/path.jsonl` to choose a specific file.

Each pending operation emits watchdog records after 5, 30, and 120 seconds.
The last open span identifies whether the run is waiting in model streaming,
tool discovery, tool execution, orchestration, or persistence. Escape handling
and abort dispatch are recorded separately.

The trace intentionally excludes prompts, keystrokes, tool arguments, tool
results, model output, credentials, and error messages. Tracing is disabled by
default, and trace write failures never interrupt a run.
