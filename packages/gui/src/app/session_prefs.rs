//! GUI session sidebar prefs: pin set and MRU timestamps (`[gui]`).

use std::collections::HashSet;

use gpui::*;

use crate::config::GuiSettings;
use crate::projections::SidebarPrefs;

use super::desktop_app::DesktopApp;

impl DesktopApp {
    pub(crate) fn sidebar_prefs(&self) -> SidebarPrefs {
        SidebarPrefs {
            pinned_session_ids: self.pinned_session_ids.clone(),
            session_last_used_at_ms: self.session_last_used_at_ms.clone(),
        }
    }

    pub(crate) fn sync_session_prefs_from_gui(&mut self, settings: &GuiSettings) {
        self.pinned_session_ids = settings.pinned_session_ids.iter().cloned().collect();
        self.session_last_used_at_ms = settings.session_last_used_at_ms.clone();
        self.prune_session_prefs();
    }

    pub(crate) fn session_prefs_into_gui(&self, settings: &mut GuiSettings) {
        settings.pinned_session_ids = self.pinned_session_ids.iter().cloned().collect();
        settings.session_last_used_at_ms = self.session_last_used_at_ms.clone();
    }

    pub(crate) fn prune_session_prefs(&mut self) {
        let known: HashSet<_> = self
            .bridge_state()
            .session_list
            .sessions
            .iter()
            .map(|s| s.session_id.clone())
            .collect();
        self.pinned_session_ids.retain(|id| known.contains(id));
        self.session_last_used_at_ms
            .retain(|id, _| known.contains(id));
    }

    pub(crate) fn bump_session_mru(&mut self, session_id: &str, cx: &mut Context<Self>) {
        let now = now_ms();
        self.session_last_used_at_ms
            .insert(session_id.to_string(), now);
        self.persist_gui_config();
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn toggle_session_pin(&mut self, session_id: &str, cx: &mut Context<Self>) {
        if self.pinned_session_ids.contains(session_id) {
            self.pinned_session_ids.remove(session_id);
        } else {
            self.pinned_session_ids.insert(session_id.to_string());
        }
        self.persist_gui_config();
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn remove_session_from_prefs(&mut self, session_id: &str) {
        self.pinned_session_ids.remove(session_id);
        self.session_last_used_at_ms.remove(session_id);
    }
}

pub(crate) fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
