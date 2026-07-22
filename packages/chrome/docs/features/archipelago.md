# Feature: Archipelago

## Overview

An **Archipelago** is the exclusive full-frame place where the user currently
is: title bar, body, and optional status bar switch together. The body is a
workspace of **islands** (split layout + focus order), not a free-form page
system separate from the island model.

Overlays are orthogonal and paint above any archipelago.

## Behavior

- Only one archipelago is active at a time.
- Navigation supports hard switch, enter-with-restore, leave-to-saved, and
  toggle between two peers.
- Router mutations report honest outcomes: unchanged vs changed (from → to).
- Each archipelago declares a default island tree and Tab focus order.
- Product-only UI state (e.g. which preferences section is open) lives in the
  app beside the router, not as a new archipelago id per subsection.

## App responsibilities

- Define product archipelago ids and default workspaces.
- Compose TitleBar / StatusBar slots for each archipelago.
- Mount island Entities for the active body’s leaves.
- Restore island keyboard focus when leaving an archipelago (app policy).

## Non-goals

- Product names for places (main editor, prefs, …) inside the kit.
- Overlay stack priority or product modal kinds.
- Animated frame transitions.
