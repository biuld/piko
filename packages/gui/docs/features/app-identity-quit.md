# GUI App Identity & Safe Quit

> Status: implemented feature contract
> Related: [GUI Workbench](workbench.md),
> [Launch](../launch.md)

## Overview

macOS App Identity & Safe Quit gives the Workbench a branded Dock / Finder
presence and a predictable exit path: closing the window quits the app, and a
busy session asks for confirmation before hostd is shut down.

## Layout

No new island or dock. The feature only changes:

- the macOS application icon shown for `Piko.app` in Dock, Finder, and About;
- window-close and Quit confirmation chrome (a modal dialog when busy).

## Behavior and interactions

### App Icon

- Opening `Piko.app` shows the vendored App Icon (not the generic binary glyph).
- The mark is an abstracted Teletubby-like robot with a glowing 'P'-shaped antenna, set against a dark gradient canvas.
- Running `cargo run -p piko-gui` as a bare binary does not install a custom
  Dock icon; branded launch requires the `.app` bundle.

### Quit on close

- Closing the Workbench window (red traffic light or equivalent) quits the app
  and shuts down the embedded hostd child.
- **Cmd+Q** and the application menu Quit item use the same busy check and
  confirm path.

### Busy confirmation

Closing or quitting is blocked with a confirm dialog when either is true:

- any live Turn is in progress (`Queued`, `Running`, `WaitingForApproval`, or
  `Cancelling`);
- there is at least one unresolved approval.

Dialog:

- Title: Quit piko?
- Body: A turn is still running or an approval is waiting.
- Buttons: Quit / Cancel

Confirm → quit (window closes; hostd shuts down). Cancel → stay in the
Workbench with no shutdown.

Idle close / idle Quit shows no dialog.

## Configuration

No new `[gui]` settings. Confirm copy is English chrome catalog only.

## In scope

- Vendored App Icon assets and a macOS `.app` bundle path that installs them
- Quit-on-close for the single Workbench window
- Shared busy predicate for window close, Cmd+Q, and menu Quit
- Confirm dialog when a Turn is in progress or an approval is unresolved

## Non-goals

- Close-window-keeps-app-in-Dock (hide-only)
- Confirm on pending interactions alone (no active turn / approval)
- Custom Dock icon for bare `cargo run` binaries
- Multi-window, light-theme icon variant, Windows / Linux icons

## Acceptance (user-visible)

- `Piko.app` shows the custom Dock / Finder icon.
- Idle red-close and idle Quit exit the app and stop hostd.
- Busy close / Quit shows the confirm dialog; Cancel keeps the window;
  Quit proceeds and shuts down hostd.
