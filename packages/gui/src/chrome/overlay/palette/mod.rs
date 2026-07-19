//! Command Palette Transient body with a root → submenu navigation stack.
//!
//! The palette root merges two sources (see `docs/host-command-catalog-design.md`):
//! - the neutral `HostCommandDescriptor` catalog fetched from hostd
//! - a small GUI-local command list for presentation actions hostd does not
//!   own (open Settings, focus/dock Sessions/Agents/Tree, quit, clear
//!   notifications)
//!
//! `model.set` / `thinking.set` host rows do not run directly — they open a
//! GUI-local nested picker; confirming a pick sends the real `SetModel` /
//! `SetThinkingLevel` `ClientIntent`.

mod render;
#[cfg(test)]
mod tests;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use piko_protocol::{HostCommandDescriptor, HostCommandGroup, HostCommandInvoke, ThinkingLevel};

actions!(
    palette,
    [PaletteSelectPrev, PaletteSelectNext, PaletteConfirm]
);

/// Host ids the GUI currently knows how to run immediately from the palette.
/// Everything else in the host catalog is listed but disabled until GUI adds
/// the matching flow (args form, confirm dialog, ...).
const GUI_RUNNABLE_HOST_IDS: &[&str] = &["session.new"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteFrameKind {
    Root,
    Models,
    Thinking,
}

/// Submenu the palette root would open next, resolved before `confirm()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSubmenu {
    Models,
    Thinking,
}

/// GUI-owned presentation commands not carried by the host catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommandId {
    OpenSettings,
    FocusSessions,
    FocusAgents,
    FocusTree,
    ClearNotifications,
    Quit,
}

fn local_commands() -> Vec<(LocalCommandId, SharedString, SharedString)> {
    vec![
        (
            LocalCommandId::FocusSessions,
            crate::t!("palette.local.sessions.title").into(),
            crate::t!("palette.local.sessions.detail").into(),
        ),
        (
            LocalCommandId::FocusAgents,
            crate::t!("palette.local.agents.title").into(),
            crate::t!("palette.local.agents.detail").into(),
        ),
        (
            LocalCommandId::FocusTree,
            crate::t!("palette.local.tree.title").into(),
            crate::t!("palette.local.tree.detail").into(),
        ),
        (
            LocalCommandId::OpenSettings,
            crate::t!("palette.local.settings.title").into(),
            crate::t!("palette.local.settings.detail").into(),
        ),
        (
            LocalCommandId::ClearNotifications,
            crate::t!("palette.local.clear_notifications.title").into(),
            crate::t!("palette.local.clear_notifications.detail").into(),
        ),
        (
            LocalCommandId::Quit,
            crate::t!("palette.local.quit.title").into(),
            crate::t!("palette.local.quit.detail").into(),
        ),
    ]
}

#[derive(Debug, Clone)]
pub(crate) struct PaletteRow {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) trailing: String,
    pub(crate) enabled: bool,
    pub(crate) action: PaletteRowAction,
}

#[derive(Debug, Clone)]
pub(crate) enum PaletteRowAction {
    /// GUI-local presentation command.
    Local(LocalCommandId),
    /// Host catalog id the GUI can run directly (see `GUI_RUNNABLE_HOST_IDS`).
    Host(String),
    EnterModels,
    EnterThinking,
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinking(ThinkingLevel),
}

/// Result of confirming the current selection.
#[derive(Debug, Clone)]
pub enum PaletteConfirmResult {
    None,
    /// Entered a submenu; keep palette open.
    StayOpen,
    RunLocal(LocalCommandId),
    RunHost(String),
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinking(ThinkingLevel),
}

pub(crate) struct PaletteFrame {
    pub(crate) kind: PaletteFrameKind,
    pub(crate) rows: Vec<PaletteRow>,
    pub(crate) filtered_ix: Vec<usize>,
    pub(crate) selected: usize,
}

impl PaletteFrame {
    fn refilter(&mut self, query: &str) {
        let q = query.trim().to_lowercase();
        self.filtered_ix = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if q.is_empty() {
                    return true;
                }
                row.title.to_lowercase().contains(&q)
                    || row.detail.to_lowercase().contains(&q)
                    || row.trailing.to_lowercase().contains(&q)
            })
            .map(|(ix, _)| ix)
            .collect();
        if self.selected >= self.filtered_ix.len() {
            self.selected = self.filtered_ix.len().saturating_sub(1);
        }
    }

    fn selected_row(&self) -> Option<&PaletteRow> {
        self.filtered_ix
            .get(self.selected)
            .and_then(|ix| self.rows.get(*ix))
    }
}

pub struct CommandPalette {
    pub(crate) filter_input: Entity<InputState>,
    catalog: Vec<HostCommandDescriptor>,
    /// Stack bottom is always Root when catalog is loaded.
    pub(crate) stack: Vec<PaletteFrame>,
    pub(crate) focus_handle: FocusHandle,
}

impl CommandPalette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::t!("palette.search.placeholder"))
        });
        cx.subscribe_in(&filter_input, window, |this, state, event, _window, cx| {
            if matches!(event, InputEvent::Change) {
                let query = state.read(cx).value().to_string();
                if let Some(frame) = this.stack.last_mut() {
                    frame.refilter(&query);
                }
                cx.notify();
            }
        })
        .detach();
        Self {
            filter_input,
            catalog: Vec::new(),
            stack: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_catalog(&mut self, catalog: Vec<HostCommandDescriptor>, cx: &mut Context<Self>) {
        self.catalog = catalog;
        // Refresh root; keep submenu if still open.
        if self.stack.is_empty()
            || self
                .stack
                .first()
                .is_some_and(|f| f.kind == PaletteFrameKind::Root)
        {
            let query = self.filter_input.read(cx).value().to_string();
            let mut root = Self::root_frame(&self.catalog);
            root.refilter(&query);
            if self.stack.is_empty() {
                self.stack.push(root);
            } else {
                self.stack[0] = root;
            }
        }
        cx.notify();
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_input.focus_handle(cx).focus(window);
    }

    pub fn reset_to_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_filter(window, cx);
        let mut root = Self::root_frame(&self.catalog);
        root.refilter("");
        self.stack.clear();
        self.stack.push(root);
        cx.notify();
    }

    /// Pop one submenu frame. Returns true if a submenu was closed.
    pub fn try_pop_submenu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        self.clear_filter(window, cx);
        if let Some(frame) = self.stack.last_mut() {
            frame.refilter("");
        }
        cx.notify();
        true
    }

    pub fn push_models(
        &mut self,
        models: Vec<(String, String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_filter(window, cx);
        let rows = if models.is_empty() {
            vec![PaletteRow {
                title: crate::t!("palette.models.empty"),
                detail: String::new(),
                trailing: String::new(),
                enabled: false,
                action: PaletteRowAction::EnterModels,
            }]
        } else {
            models
                .into_iter()
                .map(|(provider, model_id, name)| PaletteRow {
                    title: name,
                    detail: format!("{provider}/{model_id}"),
                    trailing: String::new(),
                    enabled: true,
                    action: PaletteRowAction::SetModel { provider, model_id },
                })
                .collect()
        };
        let mut frame = PaletteFrame {
            kind: PaletteFrameKind::Models,
            rows,
            filtered_ix: Vec::new(),
            selected: 0,
        };
        frame.refilter("");
        self.stack.push(frame);
        cx.notify();
    }

    pub fn push_thinking(
        &mut self,
        levels: &[ThinkingLevel],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_filter(window, cx);
        let rows = levels
            .iter()
            .map(|level| PaletteRow {
                title: level.as_str().to_string(),
                detail: crate::t!("palette.thinking.detail"),
                trailing: String::new(),
                enabled: true,
                action: PaletteRowAction::SetThinking(level.clone()),
            })
            .collect();
        let mut frame = PaletteFrame {
            kind: PaletteFrameKind::Thinking,
            rows,
            filtered_ix: Vec::new(),
            selected: 0,
        };
        frame.refilter("");
        self.stack.push(frame);
        cx.notify();
    }

    pub fn confirm(&mut self) -> PaletteConfirmResult {
        let Some(row) = self.stack.last().and_then(|f| f.selected_row()).cloned() else {
            return PaletteConfirmResult::None;
        };
        if !row.enabled {
            return PaletteConfirmResult::None;
        }
        match row.action {
            PaletteRowAction::EnterModels | PaletteRowAction::EnterThinking => {
                // DesktopApp opens the submenu with data.
                PaletteConfirmResult::StayOpen
            }
            PaletteRowAction::Local(id) => PaletteConfirmResult::RunLocal(id),
            PaletteRowAction::Host(id) => PaletteConfirmResult::RunHost(id),
            PaletteRowAction::SetModel { provider, model_id } => {
                PaletteConfirmResult::SetModel { provider, model_id }
            }
            PaletteRowAction::SetThinking(level) => PaletteConfirmResult::SetThinking(level),
        }
    }

    /// The submenu the selected root row would open (Models / Thinking), if
    /// any. DesktopApp checks this before `confirm()` so it can open the
    /// submenu with freshly fetched data instead of just seeing `StayOpen`.
    pub fn selected_root_submenu(&self) -> Option<RootSubmenu> {
        if self.stack.len() != 1 {
            return None;
        }
        match self.stack[0].selected_row()?.action {
            PaletteRowAction::EnterModels => Some(RootSubmenu::Models),
            PaletteRowAction::EnterThinking => Some(RootSubmenu::Thinking),
            _ => None,
        }
    }

    /// Replace the Models submenu rows when still open (e.g. after ListModels).
    pub fn refresh_models_if_open(
        &mut self,
        models: Vec<(String, String, String)>,
        cx: &mut Context<Self>,
    ) {
        if self.stack.last().map(|f| f.kind) != Some(PaletteFrameKind::Models) {
            return;
        }
        let rows = models
            .into_iter()
            .map(|(provider, model_id, name)| PaletteRow {
                title: name,
                detail: format!("{provider}/{model_id}"),
                trailing: String::new(),
                enabled: true,
                action: PaletteRowAction::SetModel { provider, model_id },
            })
            .collect();
        let query = self.filter_input.read(cx).value().to_string();
        let mut frame = PaletteFrame {
            kind: PaletteFrameKind::Models,
            rows,
            filtered_ix: Vec::new(),
            selected: 0,
        };
        frame.refilter(&query);
        if let Some(last) = self.stack.last_mut() {
            *last = frame;
        }
        cx.notify();
    }

    pub fn frame_title(&self) -> String {
        match self.stack.last().map(|f| f.kind) {
            Some(PaletteFrameKind::Models) => crate::t!("palette.models.title"),
            Some(PaletteFrameKind::Thinking) => crate::t!("palette.thinking.title"),
            // Root: search field is the header; no crumb title.
            _ => String::new(),
        }
    }

    fn clear_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }

    pub(crate) fn root_frame(catalog: &[HostCommandDescriptor]) -> PaletteFrame {
        let mut rows: Vec<PaletteRow> = catalog.iter().map(host_row).collect();
        rows.extend(
            local_commands()
                .into_iter()
                .map(|(id, title, detail)| PaletteRow {
                    title: title.to_string(),
                    detail: detail.to_string(),
                    trailing: String::new(),
                    enabled: true,
                    action: PaletteRowAction::Local(id),
                }),
        );
        let mut frame = PaletteFrame {
            kind: PaletteFrameKind::Root,
            rows,
            filtered_ix: Vec::new(),
            selected: 0,
        };
        frame.refilter("");
        frame
    }

    pub(crate) fn move_sel(&mut self, delta: isize) {
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        if frame.filtered_ix.is_empty() {
            return;
        }
        let len = frame.filtered_ix.len() as isize;
        frame.selected = (frame.selected as isize + delta).rem_euclid(len) as usize;
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn host_row(item: &HostCommandDescriptor) -> PaletteRow {
    let trailing = group_trailing(item.group);
    match item.id.as_str() {
        "model.set" => PaletteRow {
            title: item.title.clone(),
            detail: item.detail.clone(),
            trailing: crate::t!("palette.submenu.marker"),
            enabled: true,
            action: PaletteRowAction::EnterModels,
        },
        "thinking.set" => PaletteRow {
            title: item.title.clone(),
            detail: item.detail.clone(),
            trailing: crate::t!("palette.submenu.marker"),
            enabled: true,
            action: PaletteRowAction::EnterThinking,
        },
        id if GUI_RUNNABLE_HOST_IDS.contains(&id) => PaletteRow {
            title: item.title.clone(),
            detail: item.detail.clone(),
            trailing,
            enabled: true,
            action: PaletteRowAction::Host(item.id.clone()),
        },
        _ => PaletteRow {
            title: item.title.clone(),
            detail: disabled_detail(&item.invoke),
            trailing,
            enabled: false,
            action: PaletteRowAction::Host(item.id.clone()),
        },
    }
}

fn disabled_detail(invoke: &HostCommandInvoke) -> String {
    match invoke {
        HostCommandInvoke::Args { .. } => crate::t!("palette.disabled.needs_args"),
        HostCommandInvoke::Confirm => crate::t!("palette.disabled.needs_confirm"),
        HostCommandInvoke::Immediate => crate::t!("palette.disabled.deferred"),
    }
}

fn group_trailing(group: Option<HostCommandGroup>) -> String {
    match group {
        Some(HostCommandGroup::Session) => "session".to_string(),
        Some(HostCommandGroup::Auth) => "auth".to_string(),
        Some(HostCommandGroup::Runtime) => "runtime".to_string(),
        Some(HostCommandGroup::Model) => "model".to_string(),
        None => String::new(),
    }
}
