//! Context-menu host with chrome-owned shadow treatment.
//!
//! `gpui-component` 0.5.1 hard-codes `shadow_lg()` inside `PopupMenu`. That
//! preset has a 10 px downward offset, which reads as a heavy tail on dark
//! island surfaces. This host keeps the upstream menu entity (focus, keyboard,
//! dismissal, and menu items), clips only its built-in shadow, and paints a
//! compact chrome shadow around it.
//!
//! The clipping wrapper assumes a flat menu. Do not add submenus through this
//! host until upstream exposes popup shadow styling; nested popup content would
//! otherwise be clipped by the root menu boundary.

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, App, BoxShadow, Context, Corner, DismissEvent, Element, ElementId, Entity,
    Focusable, GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, Point, StyleRefinement, Styled, Subscription, Window,
    anchored, deferred, div, hsla, point, prelude::FluentBuilder, px,
};
use gpui_component::menu::PopupMenu;

use crate::theme::chrome_palette;

type MenuBuilder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Adds a flat context menu with piko chrome shadow treatment.
pub trait ChromeContextMenuExt: ParentElement + Styled {
    fn chrome_context_menu(
        self,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> ChromeContextMenu<Self> {
        ChromeContextMenu::new("chrome-context-menu", self).menu(build)
    }
}

impl<E: ParentElement + Styled> ChromeContextMenuExt for E {}

/// Right-click host for an upstream [`PopupMenu`].
pub struct ChromeContextMenu<E: ParentElement + Styled + Sized> {
    id: ElementId,
    element: Option<E>,
    menu: Option<MenuBuilder>,
    fallback_style: StyleRefinement,
    anchor: Corner,
}

impl<E: ParentElement + Styled> ChromeContextMenu<E> {
    fn new(id: impl Into<ElementId>, element: E) -> Self {
        Self {
            id: id.into(),
            element: Some(element),
            menu: None,
            fallback_style: StyleRefinement::default(),
            anchor: Corner::TopLeft,
        }
    }

    #[must_use]
    fn menu(
        mut self,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }

    fn with_element_state<R>(
        &mut self,
        id: &GlobalElementId,
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(&mut Self, &mut ChromeContextMenuState, &mut Window, &mut App) -> R,
    ) -> R {
        window.with_optional_element_state::<ChromeContextMenuState, _>(
            Some(id),
            |state, window| {
                let mut state = state.unwrap().unwrap_or_default();
                let result = f(self, &mut state, window, cx);
                (result, Some(state))
            },
        )
    }
}

impl<E: ParentElement + Styled> ParentElement for ChromeContextMenu<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        if let Some(element) = &mut self.element {
            element.extend(elements);
        }
    }
}

impl<E: ParentElement + Styled> Styled for ChromeContextMenu<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element
            .as_mut()
            .map(Styled::style)
            .unwrap_or(&mut self.fallback_style)
    }
}

impl<E: ParentElement + Styled + IntoElement + 'static> IntoElement for ChromeContextMenu<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct SharedState {
    menu: Option<Entity<PopupMenu>>,
    open: bool,
    position: Point<Pixels>,
    subscription: Option<Subscription>,
}

pub struct ChromeContextMenuState {
    element: Option<AnyElement>,
    shared: Rc<RefCell<SharedState>>,
}

impl Default for ChromeContextMenuState {
    fn default() -> Self {
        Self {
            element: None,
            shared: Rc::new(RefCell::new(SharedState {
                menu: None,
                open: false,
                position: Point::default(),
                subscription: None,
            })),
        }
    }
}

fn compact_menu_shadow() -> Vec<BoxShadow> {
    let alpha = if chrome_palette().is_dark() {
        0.22
    } else {
        0.14
    };
    vec![BoxShadow {
        color: hsla(0., 0., 0., alpha),
        offset: point(px(0.), px(2.)),
        blur_radius: px(8.),
        spread_radius: px(-1.),
    }]
}

fn framed_menu(menu: Entity<PopupMenu>) -> impl IntoElement {
    div()
        .rounded(px(8.))
        .shadow(compact_menu_shadow())
        .child(div().rounded(px(8.)).overflow_hidden().child(menu))
}

impl<E: ParentElement + Styled + IntoElement + 'static> Element for ChromeContextMenu<E> {
    type RequestLayoutState = ChromeContextMenuState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let anchor = self.anchor;
        self.with_element_state(
            id.expect("context menu element id"),
            window,
            cx,
            |this, state, window, cx| {
                let (position, open, menu) = {
                    let shared = state.shared.borrow();
                    (shared.position, shared.open, shared.menu.clone())
                };
                let menu_element =
                    if open && menu.as_ref().is_some_and(|menu| !menu.read(cx).is_empty()) {
                        Some(
                            deferred(
                                anchored().child(
                                    div()
                                        .w(window.bounds().size.width)
                                        .h(window.bounds().size.height)
                                        .occlude()
                                        .child(
                                            anchored()
                                                .position(position)
                                                .snap_to_window_with_margin(px(8.))
                                                .anchor(anchor)
                                                .when_some(menu, |host, menu| {
                                                    if !menu
                                                        .focus_handle(cx)
                                                        .contains_focused(window, cx)
                                                    {
                                                        menu.focus_handle(cx).focus(window);
                                                    }
                                                    host.child(framed_menu(menu))
                                                }),
                                        ),
                                ),
                            )
                            .with_priority(1)
                            .into_any(),
                        )
                    } else {
                        None
                    };

                let mut element = this
                    .element
                    .take()
                    .expect("context menu host element")
                    .children(menu_element)
                    .into_any_element();
                let layout_id = element.request_layout(window, cx);
                (
                    layout_id,
                    ChromeContextMenuState {
                        element: Some(element),
                        ..Default::default()
                    },
                )
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = &mut state.element {
            element.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = &mut state.element {
            element.paint(window, cx);
        }

        let builder = self.menu.clone();
        self.with_element_state(
            id.expect("context menu element id"),
            window,
            cx,
            |_this, state, window, _cx| {
                let shared = state.shared.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if !phase.bubble()
                        || event.button != MouseButton::Right
                        || !bounds.contains(&event.position)
                    {
                        return;
                    }

                    {
                        let mut state = shared.borrow_mut();
                        state.menu = None;
                        state.subscription = None;
                        state.position = event.position;
                        state.open = true;
                    }

                    window.defer(cx, {
                        let shared = shared.clone();
                        let builder = builder.clone();
                        move |window, cx| {
                            let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
                                if let Some(build) = builder.as_ref() {
                                    build(menu, window, cx)
                                } else {
                                    menu
                                }
                            });
                            let subscription = window.subscribe(&menu, cx, {
                                let shared = shared.clone();
                                move |_, _: &DismissEvent, window, _| {
                                    shared.borrow_mut().open = false;
                                    window.refresh();
                                }
                            });
                            let mut state = shared.borrow_mut();
                            state.menu = Some(menu);
                            state.subscription = Some(subscription);
                            window.refresh();
                        }
                    });
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::compact_menu_shadow;

    #[test]
    fn compact_shadow_has_no_long_downward_tail() {
        let shadow = compact_menu_shadow();
        assert_eq!(shadow.len(), 1);
        assert_eq!(shadow[0].offset.y, gpui::px(2.));
        assert_eq!(shadow[0].blur_radius, gpui::px(8.));
        assert_eq!(shadow[0].spread_radius, gpui::px(-1.));
    }
}
