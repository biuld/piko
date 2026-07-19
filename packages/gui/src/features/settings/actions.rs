//! Settings mutations — host `ConfigUpdate` patches and `[gui]` prefs.

use gpui::*;
use piko_client_core::ClientIntent;
use piko_protocol::ThinkingLevel;

use crate::app::desktop_app::DesktopApp;
use crate::config::HostRuntimeSettings;

impl DesktopApp {
    pub(crate) fn settings_set_model(
        &mut self,
        provider: String,
        model_id: String,
        cx: &mut Context<Self>,
    ) {
        self.bridge
            .intent(ClientIntent::SetModel { provider, model_id });
        cx.notify();
    }

    pub(crate) fn settings_set_thinking_level(
        &mut self,
        level: ThinkingLevel,
        cx: &mut Context<Self>,
    ) {
        self.bridge.intent(ClientIntent::SetThinkingLevel { level });
        cx.notify();
    }

    pub(crate) fn settings_refresh_models(&mut self, cx: &mut Context<Self>) {
        self.bridge.intent(ClientIntent::ListModels);
        cx.notify();
    }

    pub(crate) fn settings_set_hide_thinking_block(&mut self, hide: bool, cx: &mut Context<Self>) {
        self.ux_prefs.hide_thinking_block = hide;
        self.persist_gui_config();
        cx.notify();
    }

    pub(crate) fn settings_set_reduced_motion(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.ux_prefs.prefer_reduced_motion = enabled;
        self.persist_gui_config();
        cx.notify();
    }

    pub(crate) fn settings_set_compaction_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.host_runtime.compaction.enabled = enabled;
        self.bridge.update_host_config(serde_json::json!({
            "compaction": { "enabled": enabled }
        }));
        cx.notify();
    }

    pub(crate) fn settings_set_retry_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.host_runtime.retry.enabled = enabled;
        self.bridge.update_host_config(serde_json::json!({
            "retry": { "enabled": enabled }
        }));
        cx.notify();
    }

    pub(crate) fn settings_set_sandbox_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.host_runtime.sandbox.enabled = enabled;
        self.bridge.update_host_config(serde_json::json!({
            "sandbox": { "enabled": enabled }
        }));
        cx.notify();
    }

    pub(crate) fn settings_set_tools_restricted(
        &mut self,
        restricted: bool,
        cx: &mut Context<Self>,
    ) {
        self.host_runtime.active_tool_names = if restricted { Some(Vec::new()) } else { None };
        let patch = if restricted {
            serde_json::json!({ "active-tool-names": [] })
        } else {
            serde_json::json!({ "active-tool-names": serde_json::Value::Null })
        };
        self.bridge.update_host_config(patch);
        cx.notify();
    }

    pub(crate) fn settings_open_config_dir(&self) {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        let dir = std::path::PathBuf::from(home).join(".piko");
        let _ = std::process::Command::new("open").arg(dir).spawn();
    }

    pub(crate) fn sync_host_runtime_config(&mut self) {
        let Some(value) = self.bridge.host_config().cloned() else {
            return;
        };
        let fingerprint = value.to_string();
        if self.host_config_fingerprint.as_ref() == Some(&fingerprint) {
            return;
        }
        match serde_json::from_value::<HostRuntimeSettings>(value) {
            Ok(settings) => {
                self.host_runtime = settings;
                self.host_config_fingerprint = Some(fingerprint);
            }
            Err(error) => {
                log::warn!("invalid host settings ignored: {error}");
            }
        }
    }
}
