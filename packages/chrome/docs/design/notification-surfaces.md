# Notification Surfaces Design

> Feature contract: [Notification Surfaces](../features/notification-surfaces.md)

Notification presentation is an L3 composite component. It depends only on
GPUI interaction primitives plus chrome theme, typography, metrics, and icons.

The public module provides a semantic severity enum, responsive notification
surface geometry, panel and row specifications, and rendering helpers.
Callbacks receive GPUI events but no application messages. The panel frame owns
elevated surface styling, maximum size, scrolling, and event occlusion. The
surface layout owns title-bar/gutter offsets, preferred panel dimensions, and a
stable toast-stack wrapper. Applications provide the viewport and own the
click-away layer because open/close policy remains a product concern.

Rows use severity tokens for a small leading status mark rather than tinting the
whole card. Text follows existing Meta, Label, and Body roles. Row identity is
caller-provided so applications can reconcile bounded stores without product
ids entering the kit.

## Public integration API

The notification module exposes four cohesive entry points:

| API | Responsibility |
|---|---|
| Bell specification and renderer | Active/unread presentation and caller-owned toggle callback |
| Toast specification plus push/clear helpers | Semantic severity mapped into GPUI Component notification lifecycle |
| Toast-layer renderer | Reads the installed GPUI Component Root and applies the stable Chrome anchor |
| Center-layer renderer | Full-window click-away target plus the standard panel anchor |

The API deliberately does not expose a singleton host, event bus, record store,
or product command type. An application may keep records in an Entity, plain
state, or another architecture; it projects that state into the presentation
specifications at render time.
