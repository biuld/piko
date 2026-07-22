# Feature: Island runtime

## Overview

An **island** is the basic unit of multi-pane chrome: independent GPUI entity,
own scroll/local UI, focus ring ownership, and directed messaging to a host.
Islands do not hold or mutate sibling island entities.

## Behavior

- Each island paints through shared panel chrome (optional header, ready /
  loading / empty / custom body, optional scroll viewport, focus ring).
- Chrome focus ownership (which island shows the ring) is separate from
  keyboard caret placement inside the island (Activate vs Claimed).
- A focus table registers heterogeneous island entities by app-defined id;
  handoff fails safely if the id is unknown (ring left unchanged).
- Cross-island work is directed: island → deferred host message → host
  updates projections or focus; not a global broadcast bus.
- Focus-related product messages map into a small chrome focus intent layer;
  domain variants stay in the app.

## App responsibilities

- Implement the island view contract on each entity.
- Register every focusable island and assert table coverage against focus order.
- Implement the host message sink; always emit via deferred schedule.
- Map product messages to focus intents where needed; handle domain intents
  after chrome routes.

## Non-goals

- Product island ids or domain message catalogs inside the kit.
- In-list row keyboard (see [list-keyboard](list-keyboard.md)).
- Data projection / backend bridges.
