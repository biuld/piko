//! Settings kit — value-aware settings navigation and choice lists.
//!
//! Product: `docs/features/settings.md`
//! Paint: thin map onto [`SelectableItem`] + [`render_selectable_list_with_pane`](crate::ui::components::selectable_list::render_selectable_list_with_pane)
//! (Standard [`PaneSpec`](crate::ui::components::pane::PaneSpec)).
//! (via `paint`); chrome from shared Pane.
//! Feedback: Selected (bg + accent on Settings rows) ≠ Active (`●`); effect badges use `warning`.

mod paint;

use ratatui::{Frame, layout::Rect};

use crate::{theme::Theme, ui::components::selectable_list::SelectableList};

/// Latency-to-effect class for a setting key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EffectClass {
    #[default]
    Live,
    Presentation,
    RestartHostd,
}

impl EffectClass {
    pub fn badge_label(self) -> Option<&'static str> {
        match self {
            Self::Live | Self::Presentation => None,
            Self::RestartHostd => Some("restart hostd"),
        }
    }
}

/// Confirm result from the settings nav stack.
#[derive(Clone, Debug)]
pub enum SettingConfirmResult<T: Clone> {
    /// Enter drilled into a section/branch.
    Drilled,
    /// User chose an option; surface should apply `T`.
    Apply(T),
    None,
}

/// A single exclusive option in a choice list.
#[derive(Clone, Debug)]
pub struct SettingOption<T: Clone> {
    pub label: String,
    pub detail: String,
    pub action: T,
    pub is_active: bool,
}

/// Exclusive choice page (boolean, enum, or preset list).
#[derive(Clone, Debug)]
pub struct SettingChoiceList<T: Clone> {
    pub title: String,
    pub effect: EffectClass,
    pub options: Vec<SettingOption<T>>,
}

/// Intermediate or root section with children.
#[derive(Clone, Debug)]
pub struct SettingSection<T: Clone> {
    pub title: String,
    /// Data-driven current value summary (required when the section owns a value).
    pub value_summary: String,
    pub effect: EffectClass,
    pub body: SettingBody<T>,
    /// Optional domain chunk label at catalog root (e.g. "Thinking").
    /// Consecutive sections with the same group share one painted header.
    pub group: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SettingBody<T: Clone> {
    /// Nested sections (folder).
    Branch(Vec<SettingSection<T>>),
    /// Exclusive options.
    Choice(SettingChoiceList<T>),
}

#[derive(Clone)]
pub(super) enum FrameContent<T: Clone> {
    Sections(Vec<SettingSection<T>>),
    Choice(SettingChoiceList<T>),
}

pub(super) struct NavFrame<T: Clone> {
    pub title: String,
    pub content: FrameContent<T>,
    pub list: SelectableList<()>,
}

/// Settings navigation stack (depth + current frame selection).
pub struct SettingsNavStack<T: Clone> {
    frames: Vec<NavFrame<T>>,
}

impl<T: Clone> SettingsNavStack<T> {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    #[allow(dead_code)] // useful for surface guards
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[allow(dead_code)] // useful for surface guards
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    pub fn at_root(&self) -> bool {
        self.frames.len() <= 1
    }

    /// Open catalog as the sole root frame.
    pub fn open_catalog(&mut self, title: impl Into<String>, sections: Vec<SettingSection<T>>) {
        self.frames.clear();
        self.push_sections(title, sections);
    }

    /// Open a single choice list as the root (e.g. thinking picker).
    pub fn open_choice(&mut self, choice: SettingChoiceList<T>) {
        self.frames.clear();
        self.push_choice(choice);
    }

    fn push_sections(&mut self, title: impl Into<String>, sections: Vec<SettingSection<T>>) {
        let len = sections.len();
        self.frames.push(NavFrame {
            title: title.into(),
            content: FrameContent::Sections(sections),
            list: SelectableList::new(vec![(); len]),
        });
    }

    fn push_choice(&mut self, choice: SettingChoiceList<T>) {
        let len = choice.options.len();
        let title = choice.title.clone();
        self.frames.push(NavFrame {
            title,
            content: FrameContent::Choice(choice),
            list: SelectableList::new(vec![(); len]),
        });
    }

    pub fn pop(&mut self) -> bool {
        self.frames.pop();
        !self.frames.is_empty()
    }

    fn matches(
        filter: &str,
        primary: &str,
        detail: &str,
        badge: Option<&str>,
        group: Option<&str>,
    ) -> bool {
        if filter.is_empty() {
            return true;
        }
        let f = filter.to_lowercase();
        primary.to_lowercase().contains(&f)
            || detail.to_lowercase().contains(&f)
            || badge.is_some_and(|b| b.to_lowercase().contains(&f))
            || group.is_some_and(|g| g.to_lowercase().contains(&f))
    }

    pub fn select_next(&mut self, filter: &str) {
        self.move_selection(filter, 1);
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.move_selection(filter, -1);
    }

    fn move_selection(&mut self, filter: &str, delta: i32) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        let filtered: Vec<usize> = match &frame.content {
            FrameContent::Sections(sections) => sections
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    Self::matches(
                        filter,
                        &s.title,
                        &s.value_summary,
                        s.effect.badge_label(),
                        s.group.as_deref(),
                    )
                })
                .map(|(i, _)| i)
                .collect(),
            FrameContent::Choice(choice) => choice
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    Self::matches(
                        filter,
                        &o.label,
                        &o.detail,
                        choice.effect.badge_label(),
                        None,
                    )
                })
                .map(|(i, _)| i)
                .collect(),
        };
        if filtered.is_empty() {
            return;
        }
        let pos = filtered
            .iter()
            .position(|&i| i == frame.list.selected)
            .unwrap_or(0);
        let next = if delta > 0 {
            (pos + 1).min(filtered.len() - 1)
        } else {
            pos.saturating_sub(1)
        };
        frame.list.selected = filtered[next];
    }

    pub fn reset_selection(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.list.selected = 0;
        }
    }

    pub fn confirm(&mut self, filter: &mut String) -> SettingConfirmResult<T> {
        enum Pending<T: Clone> {
            Branch {
                title: String,
                children: Vec<SettingSection<T>>,
            },
            Choice(SettingChoiceList<T>),
            Apply(T),
        }

        let pending = {
            let Some(frame) = self.frames.last() else {
                return SettingConfirmResult::None;
            };
            match &frame.content {
                FrameContent::Sections(sections) => {
                    let filtered: Vec<usize> = sections
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| {
                            Self::matches(
                                filter,
                                &s.title,
                                &s.value_summary,
                                s.effect.badge_label(),
                                s.group.as_deref(),
                            )
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if filtered.is_empty() {
                        return SettingConfirmResult::None;
                    }
                    let pos = filtered
                        .iter()
                        .position(|&i| i == frame.list.selected)
                        .unwrap_or(0)
                        .min(filtered.len() - 1);
                    let section = sections[filtered[pos]].clone();
                    match section.body {
                        SettingBody::Branch(children) => Some(Pending::Branch {
                            title: section.title,
                            children,
                        }),
                        SettingBody::Choice(choice) => Some(Pending::Choice(choice)),
                    }
                }
                FrameContent::Choice(choice) => {
                    let filtered: Vec<usize> = choice
                        .options
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| {
                            Self::matches(
                                filter,
                                &o.label,
                                &o.detail,
                                choice.effect.badge_label(),
                                None,
                            )
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if filtered.is_empty() {
                        return SettingConfirmResult::None;
                    }
                    let pos = filtered
                        .iter()
                        .position(|&i| i == frame.list.selected)
                        .unwrap_or(0)
                        .min(filtered.len() - 1);
                    Some(Pending::Apply(choice.options[filtered[pos]].action.clone()))
                }
            }
        };

        let Some(pending) = pending else {
            return SettingConfirmResult::None;
        };
        match pending {
            Pending::Branch { title, children } => {
                filter.clear();
                self.push_sections(title, children);
                SettingConfirmResult::Drilled
            }
            Pending::Choice(choice) => {
                filter.clear();
                self.push_choice(choice);
                SettingConfirmResult::Drilled
            }
            Pending::Apply(action) => SettingConfirmResult::Apply(action),
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, filter: &str, theme: &Theme) {
        let Some(nav) = self.frames.last() else {
            return;
        };
        paint::paint_frame(frame, area, nav, self.at_root(), filter, theme);
    }
}

impl<T: Clone> Default for SettingsNavStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drill_then_apply() {
        let mut stack = SettingsNavStack::new();
        stack.open_catalog(
            "settings",
            vec![SettingSection {
                title: "Retries".into(),
                value_summary: "On".into(),
                effect: EffectClass::Live,
                group: Some("Runtime".into()),
                body: SettingBody::Choice(SettingChoiceList {
                    title: "API Retries".into(),
                    effect: EffectClass::Live,
                    options: vec![
                        SettingOption {
                            label: "On".into(),
                            detail: "retry".into(),
                            action: true,
                            is_active: true,
                        },
                        SettingOption {
                            label: "Off".into(),
                            detail: "no".into(),
                            action: false,
                            is_active: false,
                        },
                    ],
                }),
            }],
        );
        let mut filter = String::new();
        assert!(matches!(
            stack.confirm(&mut filter),
            SettingConfirmResult::Drilled
        ));
        assert!(!stack.at_root());
        stack.select_next("");
        match stack.confirm(&mut filter) {
            SettingConfirmResult::Apply(v) => assert!(!v),
            other => panic!("expected apply, got {other:?}"),
        }
    }
}
