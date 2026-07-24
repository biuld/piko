//! Secondary-click host and deferred anchored menu layer.

use std::{cell::RefCell, rc::Rc};

use gpui::*;

use super::registry::{ActiveContextMenu, ContextMenuRegistry};
use super::{ContextMenu, ContextMenuSpec};

type MenuBuilder = Rc<dyn Fn(ContextMenuRequest, &mut Window, &mut App) -> ContextMenuSpec>;

#[derive(Clone, Copy)]
pub struct ContextMenuRequest {
    pub position: Point<Pixels>,
}

pub trait ContextMenuExt: ParentElement + Styled {
    fn context_menu(
        self,
        build: impl Fn(ContextMenuRequest, &mut Window, &mut App) -> ContextMenuSpec + 'static,
    ) -> ContextMenuHost<Self> {
        ContextMenuHost::new("piko-context-menu-host", self, build)
    }
}

impl<E: ParentElement + Styled> ContextMenuExt for E {}

pub struct ContextMenuHost<E: ParentElement + Styled + Sized> {
    id: ElementId,
    element: Option<E>,
    builder: MenuBuilder,
    fallback_style: StyleRefinement,
}

impl<E: ParentElement + Styled> ContextMenuHost<E> {
    fn new(
        id: impl Into<ElementId>,
        element: E,
        build: impl Fn(ContextMenuRequest, &mut Window, &mut App) -> ContextMenuSpec + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            element: Some(element),
            builder: Rc::new(build),
            fallback_style: StyleRefinement::default(),
        }
    }

    fn with_state<R>(
        &mut self,
        id: &GlobalElementId,
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(&mut Self, &mut ContextMenuHostState, &mut Window, &mut App) -> R,
    ) -> R {
        window.with_optional_element_state::<ContextMenuHostState, _>(Some(id), |state, window| {
            let mut state = state.unwrap().unwrap_or_default();
            let result = f(self, &mut state, window, cx);
            (result, Some(state))
        })
    }
}

impl<E: ParentElement + Styled> ParentElement for ContextMenuHost<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        if let Some(element) = &mut self.element {
            element.extend(elements);
        }
    }
}

impl<E: ParentElement + Styled> Styled for ContextMenuHost<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element
            .as_mut()
            .map(Styled::style)
            .unwrap_or(&mut self.fallback_style)
    }
}

impl<E: ParentElement + Styled + IntoElement + 'static> IntoElement for ContextMenuHost<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[derive(Default)]
struct SharedState {
    menu: Option<Entity<ContextMenu>>,
    open: bool,
    position: Point<Pixels>,
    subscription: Option<Subscription>,
}

pub struct ContextMenuHostState {
    element: Option<AnyElement>,
    shared: Rc<RefCell<SharedState>>,
}

impl Default for ContextMenuHostState {
    fn default() -> Self {
        Self {
            element: None,
            shared: Rc::new(RefCell::new(SharedState::default())),
        }
    }
}

impl<E: ParentElement + Styled + IntoElement + 'static> Element for ContextMenuHost<E> {
    type RequestLayoutState = ContextMenuHostState;
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.with_state(
            id.expect("context menu host id"),
            window,
            cx,
            |this, state, window, cx| {
                let (open, position, menu) = {
                    let shared = state.shared.borrow();
                    (shared.open, shared.position, shared.menu.clone())
                };
                let layer = if open {
                    menu.map(|menu| {
                        if !menu.focus_handle(cx).contains_focused(window, cx) {
                            menu.focus_handle(cx).focus(window);
                        }
                        deferred(
                            anchored().child(
                                div()
                                    .w(window.bounds().size.width)
                                    .h(window.bounds().size.height)
                                    .occlude()
                                    .child(
                                        anchored()
                                            .position(position)
                                            .anchor(Corner::TopLeft)
                                            .snap_to_window_with_margin(px(8.))
                                            .child(menu),
                                    ),
                            ),
                        )
                        .with_priority(1)
                        .into_any()
                    })
                } else {
                    None
                };
                let mut element = this
                    .element
                    .take()
                    .expect("context menu trigger")
                    .children(layer)
                    .into_any_element();
                let layout_id = element.request_layout(window, cx);
                (
                    layout_id,
                    ContextMenuHostState {
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
        _: Bounds<Pixels>,
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
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = &mut state.element {
            element.paint(window, cx);
        }
        let builder = self.builder.clone();
        self.with_state(
            id.expect("context menu host id"),
            window,
            cx,
            |_this, state, window, _| {
                let shared = state.shared.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if !phase.bubble()
                        || !is_secondary_click(event)
                        || !bounds.contains(&event.position)
                    {
                        return;
                    }
                    cx.stop_propagation();
                    window.prevent_default();
                    let request = ContextMenuRequest {
                        position: event.position,
                    };
                    let shared = shared.clone();
                    let builder = builder.clone();
                    window.defer(cx, move |window, cx| {
                        let spec = builder(request, window, cx);
                        if spec.is_empty() {
                            return;
                        }
                        open_menu(spec, request.position, shared, window, cx);
                    });
                });
            },
        );
    }
}

fn is_secondary_click(event: &MouseDownEvent) -> bool {
    event.button == MouseButton::Right
        || (cfg!(target_os = "macos")
            && event.button == MouseButton::Left
            && event.modifiers.control)
}

fn open_menu(
    spec: ContextMenuSpec,
    position: Point<Pixels>,
    shared: Rc<RefCell<SharedState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let window_id = window.window_handle().window_id();
    let previous = cx.update_default_global::<ContextMenuRegistry, _>(|registry, _| {
        registry.windows.remove(&window_id)
    });
    let restore_focus = previous
        .as_ref()
        .and_then(|active| active.restore_focus.clone())
        .or_else(|| window.focused(cx));
    if let Some(previous) = previous
        && let Some(menu) = previous.menu.upgrade()
    {
        menu.update(cx, |menu, cx| menu.replace(cx));
    }

    let max_width = (window.bounds().size.width - px(16.))
        .max(px(1.))
        .min(px(320.));
    let menu = cx.new(|cx| ContextMenu::new(spec, max_width, cx));
    let subscription = window.subscribe(&menu, cx, {
        let shared = shared.clone();
        move |_, _: &DismissEvent, window, _| {
            let mut state = shared.borrow_mut();
            state.open = false;
            state.menu = None;
            state.subscription = None;
            window.refresh();
        }
    });
    cx.update_default_global::<ContextMenuRegistry, _>(|registry, _| {
        registry.windows.insert(
            window_id,
            ActiveContextMenu {
                entity_id: menu.entity_id(),
                menu: menu.downgrade(),
                restore_focus,
            },
        );
    });
    {
        let mut state = shared.borrow_mut();
        state.position = position;
        state.open = true;
        state.menu = Some(menu);
        state.subscription = Some(subscription);
    }
    window.refresh();
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, MouseButton, MouseDownEvent};

    use super::is_secondary_click;

    #[test]
    fn control_primary_is_secondary_only_on_macos() {
        let event = MouseDownEvent {
            button: MouseButton::Left,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(is_secondary_click(&event), cfg!(target_os = "macos"));
    }
}
