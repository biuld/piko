//! Floating sidebar surface: session discovery and agent hierarchy
//! (D-59 Slice 3).

use gpui::prelude::*;
use gpui::{App, IntoElement, ScrollHandle, Window, div, point, px};
use island::components::source_list::{SourceList, SourceRow, SourceSection};
use island::platform::material::WindowMaterialHost;
use island::theme::{SurfaceRole, TextRole, fill, hairline, metrics, text, tokens};

use piko_client_core::ClientState;

/// Sidebar presentation width (F-42 open question 1: one adaptive product
/// width in v1, not user-resizable).
pub const SIDEBAR_WIDTH: f32 = 264.0;

/// Narrow-window breakpoint: below this total width the persistent sidebar
/// leaves the layout (F-42 responsive sidebar).
pub const MIN_TIMELINE_WIDTH: f32 = 480.0;

pub fn uses_persistent_sidebar(window_width: f32, preference_collapsed: bool) -> bool {
    window_width >= SIDEBAR_WIDTH + MIN_TIMELINE_WIDTH && !preference_collapsed
}

/// Row identity for the navigation list. Indexes resolve against the model
/// captured at render time (painted-frame authority, cf. ADR-018).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavId {
    NewSession,
    Session(usize),
    Agent(usize),
    Settings,
}

/// Navigation sections plus the selected id for highlight.
pub struct NavModel {
    pub sections: Vec<SourceSection<NavId>>,
    /// Currently selected nav id for highlight.
    pub selected: Option<NavId>,
    /// Shell-owned keyboard cursor, independent from host selection.
    pub keyboard_focused: Option<NavId>,
}

/// Build the navigation model from the sole host projection.
pub fn nav_model(core: &ClientState, keyboard_focused: Option<NavId>) -> NavModel {
    let mut session_rows = Vec::new();
    let live_id = core.live_session.as_ref().map(|s| s.session_id.clone());
    let mut selected = None;

    for (index, summary) in core.session_list.sessions.iter().enumerate() {
        let label = summary
            .name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                summary
                    .first_message
                    .clone()
                    .map(|first| truncate_summary(&first, 48))
            })
            .unwrap_or_else(|| summary.cwd.clone());
        if live_id.as_deref() == Some(summary.session_id.as_str()) {
            selected = Some(NavId::Session(index));
        }
        session_rows.push(SourceRow::new(
            NavId::Session(index),
            format!("session-{index}"),
            label,
        ));
    }

    let mut agent_rows = Vec::new();
    let selected_agent = core
        .live_session
        .as_ref()
        .and_then(|session| session.selected_agent.clone());
    if let Some(live) = core.live_session.as_ref() {
        for (index, agent) in live.agents.iter().enumerate() {
            let label = if agent.name.is_empty() {
                agent.agent_id.clone()
            } else {
                agent.name.clone()
            };
            if selected_agent.as_deref() == Some(agent.agent_instance_id.as_str()) {
                selected = Some(NavId::Agent(index));
            }
            agent_rows.push(SourceRow::new(
                NavId::Agent(index),
                format!("agent-{index}"),
                label,
            ));
        }
    }

    let mut sections = Vec::new();
    if !session_rows.is_empty() {
        sections.push(SourceSection::unlabeled(session_rows));
    }
    if !agent_rows.is_empty() {
        sections.push(SourceSection::new("Agents", agent_rows));
    }
    sections.push(SourceSection::new(
        "Application",
        vec![
            SourceRow::new(NavId::NewSession, "new-session", "New Session"),
            SourceRow::new(NavId::Settings, "settings", "Settings"),
        ],
    ));

    NavModel {
        sections,
        selected,
        keyboard_focused,
    }
}

fn truncate_summary(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(limit).collect();
        cut.push('…');
        cut
    }
}

/// Activation handler shared across sidebar renders.
pub type NavActivate = std::rc::Rc<dyn Fn(NavId, &mut Window, &mut App)>;

/// Render the elevated floating sidebar surface around a navigation list.
pub fn render_sidebar_surface(
    model: NavModel,
    material: WindowMaterialHost,
    scroll: &ScrollHandle,
    on_activate: NavActivate,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();

    div()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .flex_shrink_0()
        .px(m.space_sm)
        .py(m.space_sm)
        .child(
            div()
                .id("piko-sidebar")
                .size_full()
                .rounded_md()
                .border_1()
                .border_color(hairline(SurfaceRole::Sidebar))
                .bg(fill(SurfaceRole::Sidebar, material))
                .text_color(t.fg_rgba())
                .child(render_sidebar_content(model, scroll, on_activate)),
        )
}

/// Source-list body for island's detached `WindowChromeFrame`, which already
/// owns the elevated sidebar surface.
pub fn render_sidebar_content(
    model: NavModel,
    scroll: &ScrollHandle,
    on_activate: NavActivate,
) -> impl IntoElement {
    div()
        .id("piko-sidebar-content")
        .size_full()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .text_color(tokens().fg_rgba())
        .child(render_nav_list(model, on_activate))
}

/// Move the source-list viewport by the minimum amount needed to reveal the
/// keyboard-focused row. SourceList owns section rendering, so the product
/// computes the matching fixed row/section geometry here.
pub fn reveal_keyboard_focus(scroll: &ScrollHandle, model: &NavModel, id: NavId) {
    let viewport = f32::from(scroll.bounds().size.height);
    if viewport <= 0.0 {
        return;
    }
    let Some((top, bottom)) = row_span(&model.sections, id) else {
        return;
    };
    let current = f32::from(scroll.offset().y);
    let max = f32::from(scroll.max_offset().y);
    let next = reveal_offset(current, viewport, top, bottom).clamp(-max, 0.0);
    scroll.set_offset(point(px(0.), px(next)));
}

const LIST_PADDING: f32 = 4.0;
const SECTION_GAP: f32 = 8.0;
const SECTION_LABEL_HEIGHT: f32 = 22.0;
const ROW_HEIGHT: f32 = 30.0;
const ROW_GAP: f32 = 2.0;

fn row_span(sections: &[SourceSection<NavId>], target: NavId) -> Option<(f32, f32)> {
    let mut y = LIST_PADDING;
    for (section_index, section) in sections.iter().enumerate() {
        if section_index > 0 {
            y += SECTION_GAP;
        }
        if section.label.is_some() {
            y += SECTION_LABEL_HEIGHT;
        }
        for (row_index, row) in section.rows.iter().enumerate() {
            if row_index > 0 {
                y += ROW_GAP;
            }
            if row.id == target {
                return Some((y, y + ROW_HEIGHT));
            }
            y += ROW_HEIGHT;
        }
    }
    None
}

fn reveal_offset(current: f32, viewport: f32, top: f32, bottom: f32) -> f32 {
    if top + current < 0.0 {
        -top
    } else if bottom + current > viewport {
        viewport - bottom
    } else {
        current
    }
}

fn render_nav_list(model: NavModel, on_activate: NavActivate) -> impl IntoElement {
    let m = metrics();
    if model.sections.is_empty() {
        return div()
            .p(m.space_sm)
            .child(
                text(TextRole::Meta)
                    .text_color(tokens().muted_fg_rgba())
                    .child("No sessions yet"),
            )
            .into_any_element();
    }
    SourceList::new(model.sections)
        .selected(model.selected)
        .keyboard_focused(model.keyboard_focused)
        .on_activate(move |id, window, app| on_activate(id, window, app))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{MIN_TIMELINE_WIDTH, SIDEBAR_WIDTH, reveal_offset, uses_persistent_sidebar};

    #[test]
    fn breakpoint_and_preference_control_persistent_sidebar() {
        let threshold = SIDEBAR_WIDTH + MIN_TIMELINE_WIDTH;
        assert!(!uses_persistent_sidebar(threshold - 1.0, false));
        assert!(uses_persistent_sidebar(threshold, false));
        assert!(!uses_persistent_sidebar(threshold + 200.0, true));
    }

    #[test]
    fn keyboard_reveal_moves_only_when_row_leaves_viewport() {
        assert_eq!(reveal_offset(0.0, 100.0, 20.0, 50.0), 0.0);
        assert_eq!(reveal_offset(0.0, 100.0, 120.0, 150.0), -50.0);
        assert_eq!(reveal_offset(-80.0, 100.0, 20.0, 50.0), -20.0);
    }
}
