# GUI / Client Core path ownership (Wave A–B)

> Status: historical implementation ownership record; Wave A–B is integrated

Base interface commit: after crate skeletons land on the working tree.
Contract: `docs/client-core-contract-baseline.md`.

| Track | Owns | Must not edit |
|---|---|---|
| Client Core | `packages/client-core/**` | `packages/gui`, workspace manifests, protocol |
| GPUI spike | `packages/gui/src/app/**`, spike view, `packages/gui/docs/**` | Client Core; do not finalize lockfile alone — propose pins |
| Transport | `packages/gui/src/transport/**` | GPUI views, Client Core state, dependency versions |
| Integration | root `Cargo.toml` / `Cargo.lock`, crate manifests after spike, bridge wiring | — |

Workers rebase onto the latest integration interface before handoff.
