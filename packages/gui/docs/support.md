# Support notes for piko-gui

## Smoke or spawn failures

1. Confirm `piko-hostd` resolves: set `PIKO_HOSTD` or build the workspace so the
   binary sits on the discovery path ([`dependency-pins.md`](dependency-pins.md)).
2. Re-run `cargo test -p piko-gui hostd_smoke -- --nocapture` and read skip
   reasons (missing binary vs empty model catalog).
3. Check `last_error` toasts / Activity Center after a failed open or submit.

## Pins and upgrades

GPUI crates are exact-version pinned. Follow the upgrade procedure in
[`dependency-pins.md`](dependency-pins.md); never track moving `main`.

## Checklists

- Manual GPUI/IME: [`manual-ux-checklist.md`](manual-ux-checklist.md)
- M4 release gate: [`release-checklist.md`](release-checklist.md)
- Product limits: [`known-limitations.md`](known-limitations.md)
- Launch: [`launch.md`](launch.md)
