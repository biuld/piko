# Notification Surfaces

## Overview

Notification surfaces provide reusable presentation for a compact notification
history panel and its rows. Applications supply notification data, unread
policy, localization, and callbacks through one presentation API.

## Behavior

- The floating panel is intended for top-right title-bar actions and does not
  dim the application surface.
- The panel has a compact header, a bounded scrolling body, and an empty state.
- Rows support informational, success, warning, and error severity, primary and
  secondary text, time metadata, and an optional remove action.
- Pointer events inside the panel do not fall through to an application's
  click-away layer.
- Panel geometry uses chrome theme tokens and remains usable in narrow windows.
- Toast stacks use a stable top-right position below the title bar; opening a
  product notification center never requires moving the stack around the
  window.
- The API provides a bell with unread state, toast push/clear/render helpers,
  and a click-away notification-center layer so applications do not need to
  assemble GPUI Component notification plumbing themselves.

## App responsibilities

- Own records, ordering, retention, unread state, persistence, and localization.
- Provide the current viewport and click-away and Escape behavior.
- Route row and header callbacks into product actions.
- Supply toast content and severity; the API integrates GPUI Component's
  animation, dismissal, and expiry lifecycle.
- Decide whether to show, suppress, or clear toasts while a history panel is
  open.

## Non-goals

- A global notification service or event bus.
- Notification history or unread-state storage.
- Product notification categories or domain navigation.
- Overlay stack priority, focus restoration, or persistence.
