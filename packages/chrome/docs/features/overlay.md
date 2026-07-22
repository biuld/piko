# Feature: Overlay

## Overview

Overlays are temporary elevated panels above the active archipelago (dialogs,
palettes, confirms). The kit paints backdrop and panel geometry; the app owns
stack priority and product modal kinds.

## Behavior

- Preferred width is supplied by the app; the kit clamps width and max height
  to the viewport with safe margins and scaled top padding.
- Panel body scrolls inside the max-height box.
- Open/close focus lifecycle is tracked so restore can run only when a session
  actually opened.
- Backdrop may optionally dismiss on click (app policy per kind).
- Pointer hits on the panel do not fall through to the archipelago.

## App responsibilities

- Pass viewport size when rendering.
- Own overlay stack order and Escape policy.
- On open: save island focus if needed, begin focus session, focus a control
  inside the panel.
- On close: end focus session and restore when indicated.
- Provide panel title and body content.

## Non-goals

- Product prompt / catalog / approval semantics.
- Guaranteed Tab-cycle trap until platform support is explicit.
