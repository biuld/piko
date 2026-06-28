# hostd Migration Status

This page used to track the TypeScript host-runtime to Rust hostd migration.
That checklist is no longer the right planning unit: many items it marked as
missing are now partially or fully wired, while the remaining risk is mostly in
runtime concurrency, protocol semantics, and state ownership.

Use the current plan instead:

- [hostd Global Plan](./hostd-global-plan.md)
- [TUI / Host Boundary](./tui-host-boundary.md)
- [hostd / orchd Runtime Boundary Correction](./hostd-orchd-runtime-boundary.md)

Historical migration notes should not be used as implementation guidance unless
they have been revalidated against current code.
