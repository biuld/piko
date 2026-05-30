# piko

Stateless engine abstraction for LLM-powered agent runtimes.

## Structure

- `packages/engine-protocol` — Shared protocol types
- `packages/engine-native` — In-process stateless engine
- `packages/engine-remote` — JSON-RPC remote engine client
- `packages/host-runtime` — Host scheduler, session store, TUI

## Development

```bash
npm install
npm run check
```
