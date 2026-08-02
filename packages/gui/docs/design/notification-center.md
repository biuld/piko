# Notification Center Design

> Status: implementation design
> Feature contract: [Notification Center](../features/notification-center.md)
> Related: [GUI Overlay Stack](overlay-stack.md)

## 1. Responsibilities

| Owner | Owns | Does not own |
|---|---|---|
| `island` | notification center store (history, unread, coalescing, toast queue, native macOS delivery), severity, compact floating-panel/row presentation, responsive panel geometry, and stable toast-stack placement | product notice types, retention bounds, localization, product copy, commands |
| `features/notifications` | piko notice type implementing `NotificationMessage`, bounded retention, localized time labels, panel body, remove/clear wiring | window title-bar placement, host runtime state, native delivery plumbing |
| GUI shell | bell placement in each TitleBar | notification data or mutations |
| `DesktopApp` wiring | one notification emission path, toast policy (suppress while panel open), Escape routing | reusable notification visuals and GPUI toast plumbing |
| GPUI Component Root | visible toast animation and five-second auto-hide | notification history |

No protocol, client-core, hostd, settings, or persistence changes are required.

## 2. State and data flow

Every GUI notification call is projected first into a piko `AppNotification`
that implements island's `NotificationMessage` contract (stable notice id,
severity, title, message, and a localized time label). The island
`NotificationCenterStore` owns history ordering, unread state, coalescing, the
queued toast list, and OS delivery. Piko keeps the store bounded to the 100
most recent rows (`push_bounded`) because retention is an app responsibility.
Notice ids are content-derived, so consecutive identical notices refresh one
history row instead of appending duplicates.

Opening the panel marks the current store read and clears visible toasts. A
notification emitted while the panel is open is inserted as read without
creating a duplicate toast — its queued fallback toast is dropped instead of
stacking behind the panel. Otherwise it raises the unread marker and flushes
the toast queue.
Removing history does not attempt to identify and remove one already-visible
GPUI toast. Clear All clears both the store and the complete GPUI toast layer.

Delivery is native-first: the store routes final notices to the configured
`OsNotificationCenter` and drops queued in-window toasts whenever the native
center reports availability (bundled app plus granted permission). Un-bundled
development binaries report native-unavailable and fall back to in-window
toasts. The existing bridge error fingerprint still deduplicates repeated
connection and host errors before they enter the store.

## 3. Surface composition

The bell is a shell-owned title-bar action shared by Workbench and Settings.
It emits a primitive app action and reads only `panel_open` and `has_unread`
presentation flags supplied by the composition root.

The panel is a non-modal root layer anchored to the top-right below the title
bar. A transparent click-away layer closes it, while the panel stops pointer
propagation. It is intentionally separate from `OverlayHost`: it has no dimmed
backdrop, does not replace Primary Surface focus, and does not participate in
HostPrompt priority. Opening a modal overlay closes Notification Center.

Escape closes Notification Center before falling through to the existing
Transient, LocalConfirm, and Sheet policy. The panel does not move keyboard
focus on open, so no focus-restore session is needed.

Toasts stay in the GPUI Component notification root layer. Chrome wraps that
layer at one stable top-right position using its title-bar and gutter metrics so
toasts never cover the bell or Settings action. Opening the panel clears and
suppresses toasts instead of moving them around the window.

## 4. Chrome boundary

The chrome kit exposes generic notification presentation and behavior under
`components::notification`: a product-free `NotificationCenterStore` and
`NotificationMessage` contract, severity-to-token mapping, responsive surface
geometry, a stable toast-stack wrapper, a floating panel frame, and compact
notification row/empty-state helpers. APIs accept viewport size,
caller-provided ids, text, relative-time labels, and callbacks. They contain no
piko product types, localization keys, or Activity Center concepts; piko maps
its own notices onto the `NotificationMessage` accessors.

The same public API renders the unread bell, pushes and clears GPUI Component
toasts, mounts the anchored toast layer, and mounts the center click-away layer.
GUI code therefore never maps Chrome severity to a second toast enum or reaches
into GPUI Component's notification Root directly.

## 5. Tradeoffs

- Reusing GPUI Component toasts preserves working animation and auto-hide while
  the product-owned store supplies stable history.
- Suppressing duplicate toasts while the panel is open prevents a second copy
  from competing with the history the user is already viewing.
- Native-first delivery gives the app system notifications for free on macOS
  while keeping in-window toasts as a safe fallback for development and
  permission-denied states.
- A separate lightweight floating layer avoids changing modal overlay priority
  or focus semantics.
- A bounded in-memory store satisfies the first release and leaves persistence
  and navigation metadata as additive future work.
