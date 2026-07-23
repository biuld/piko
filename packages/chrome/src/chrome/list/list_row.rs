//! Shared flat **selectable list row** paint (nav lists, simple pickers).
//!
//! Chrome owns elevated/selected background, keyboard focus ring, and density.
//! Apps supply labels, colors, and click handlers (product domain stays out).
//!
//! For trees / hierarchical rows use [`super::tree_list`] instead.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::{metrics, tokens};

/// Click handler for a flat list row.
pub type ListClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One flat list row (no depth guides / expand disclosure).
#[derive(Debug, Clone)]
pub struct ListRowSpec {
    pub id: SharedString,
    /// Elevated background (active selection / current item).
    pub selected: bool,
    /// Focus-visible ring; independent of [`Self::selected`].
    pub keyboard_focused: bool,
    pub label: SharedString,
    /// Overrides default / selected label color when set.
    pub label_color: Option<Rgba>,
    /// When false, row paints muted and apps may still attach click (or not).
    pub enabled: bool,
}

impl ListRowSpec {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            selected: false,
            keyboard_focused: false,
            label: label.into(),
            label_color: None,
            enabled: true,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn keyboard_focused(mut self, keyboard_focused: bool) -> Self {
        self.keyboard_focused = keyboard_focused;
        self
    }

    pub fn label_color(mut self, color: Rgba) -> Self {
        self.label_color = Some(color);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Pure paint flags derived from a [`ListRowSpec`] (unit-testable without a window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListRowChrome {
    pub elevated_bg: bool,
    pub focus_ring: bool,
    pub muted: bool,
}

/// Resolve chrome paint flags from a row spec.
pub fn list_row_chrome(spec: &ListRowSpec) -> ListRowChrome {
    ListRowChrome {
        elevated_bg: spec.selected,
        focus_ring: spec.keyboard_focused,
        muted: !spec.enabled,
    }
}

/// Render a vertical stack of flat selectable rows.
pub fn render_list(
    rows: impl IntoIterator<Item = (ListRowSpec, ListClickHandler)>,
) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .children(
            rows.into_iter()
                .map(|(spec, on_click)| render_list_row(spec, on_click)),
        )
        .py(m.space_sm)
        .px(m.space_xs)
}

/// Paint one flat list row.
pub fn render_list_row(spec: ListRowSpec, on_click: ListClickHandler) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let chrome = list_row_chrome(&spec);
    let label_color = spec.label_color.unwrap_or_else(|| {
        if spec.selected {
            t.accent_rgba()
        } else {
            t.fg_rgba()
        }
    });
    let row_id = spec.id.clone();

    let enabled = spec.enabled;
    let row = div()
        .id(row_id)
        .h(px(32.))
        .w_full()
        .px(m.tool_row_inset)
        .rounded_sm()
        .flex()
        .items_center()
        .when(chrome.elevated_bg, |d| d.bg(t.elevated_rgba()))
        .when(chrome.focus_ring, |d| {
            d.border_1().border_color(t.ring_rgba())
        })
        .when(chrome.muted, |d| d.opacity(0.45))
        .child(
            crate::theme::label_text(spec.selected)
                .text_color(label_color)
                .child(spec.label),
        );

    if enabled {
        row.cursor_pointer()
            .hover(|style| style.bg(t.elevated_rgba()))
            .on_click(move |ev, window, cx| on_click(ev, window, cx))
            .into_any_element()
    } else {
        row.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Avoid `use super::*` — pulls GPUI into #[test] expansion (recursion limit).
    use super::{ListRowChrome, ListRowSpec, list_row_chrome};

    #[test]
    fn chrome_flags_selected_and_keyboard_independent() {
        let base = ListRowSpec::new("a", "Alpha");
        assert_eq!(
            list_row_chrome(&base),
            ListRowChrome {
                elevated_bg: false,
                focus_ring: false,
                muted: false,
            }
        );

        let selected = base.clone().selected(true);
        assert!(list_row_chrome(&selected).elevated_bg);
        assert!(!list_row_chrome(&selected).focus_ring);

        let kb = base.clone().keyboard_focused(true);
        assert!(!list_row_chrome(&kb).elevated_bg);
        assert!(list_row_chrome(&kb).focus_ring);

        let both = ListRowSpec::new("b", "Beta")
            .selected(true)
            .keyboard_focused(true);
        let c = list_row_chrome(&both);
        assert!(c.elevated_bg && c.focus_ring && !c.muted);
    }

    #[test]
    fn disabled_row_is_muted() {
        let spec = ListRowSpec::new("x", "Disabled").enabled(false);
        assert!(list_row_chrome(&spec).muted);
    }

    #[test]
    fn builder_sets_identity_fields() {
        let spec = ListRowSpec::new("nav-general", "General").selected(true);
        assert_eq!(spec.id.as_ref(), "nav-general");
        assert_eq!(spec.label.as_ref(), "General");
        assert!(spec.selected);
        assert!(spec.enabled);
    }
}
