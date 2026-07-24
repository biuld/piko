# Notification Center Design

> Status: implementation design
> Feature contract: [Notification Center](../features/notification-center.md)
> Related: [GUI Overlay Stack](overlay-stack.md)

## 1. Responsibilities

| Owner | Owns | Does not own |
|---|---|---|
| `piko-chrome` | reusable notification severity, compact floating-panel/row presentation, responsive panel geometry, and stable toast-stack placement | notification records, unread policy, product copy, commands |
| `features/notifications` | bounded in-memory records, unread state, panel body, remove/clear behavior | window title-bar placement, host runtime state |
| GUI shell | bell placement in each TitleBar | notification data or mutations |
| `DesktopApp` wiring | one notification emission path, history policy, Escape routing | reusable notification visuals and GPUI toast plumbing |
| GPUI Component Root | visible toast animation and five-second auto-hide | notification history |

No protocol, client-core, hostd, settings, or persistence changes are required.

## 2. State and data flow

Every GUI notification call is projected first into a window-local notification
record. When the panel is closed, it is then forwarded to GPUI Component's
toast layer. The record contains a
monotonic id, severity, title, message, creation time, and read state. The store
retains at most 100 records in newest-first order.

Opening the panel marks the current store read and clears visible toasts. A
notification emitted while the panel is open is inserted as read without
creating a duplicate toast; otherwise it raises the unread marker and displays
the toast.
Removing history does not attempt to identify and remove one already-visible
GPUI toast. Clear All clears both the store and the complete GPUI toast layer.

The existing bridge error fingerprint remains responsible for deduplicating
repeated connection and host errors before they enter the store.

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

The chrome kit exposes generic notification presentation under
`components::notification`: severity-to-token mapping, responsive surface
geometry, a stable toast-stack wrapper, a floating panel frame, and compact
notification row/empty-state helpers. APIs accept viewport size,
caller-provided ids, text, relative-time labels, and callbacks. They contain no
piko product types, localization keys, history store, or Activity Center
concepts.

The same public API renders the unread bell, pushes and clears GPUI Component
toasts, mounts the anchored toast layer, and mounts the center click-away layer.
GUI code therefore never maps Chrome severity to a second toast enum or reaches
into GPUI Component's notification Root directly.

## 5. Tradeoffs

- Reusing GPUI Component toasts preserves working animation and auto-hide while
  the product-owned store supplies stable history.
- Suppressing duplicate toasts while the panel is open prevents a second copy
  from competing with the history the user is already viewing.
- A separate lightweight floating layer avoids changing modal overlay priority
  or focus semantics.
- A bounded in-memory store satisfies the first release and leaves persistence
  and navigation metadata as additive future work.
