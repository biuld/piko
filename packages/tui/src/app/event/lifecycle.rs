use super::*;

impl AppState {
    pub(super) fn apply_approval(&mut self, event: piko_protocol::ApprovalEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::ApprovalEvent::Requested {
                session_id,
                agent_instance_id,
                approval_id,
                tool_name,
                tool_args,
                prompt,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.approvals.push(PendingApproval {
                    id: approval_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    tool_name: tool_name.clone(),
                    args: tool_args,
                    prompt,
                    selected_idx: 0,
                });
                self.status = format!("approval requested for {tool_name}");
                self.notifications.push_with(
                    crate::features::notifications::NoticeScope::Session(session_id.clone()),
                    NotificationLevel::Warning,
                    crate::features::notifications::NoticePolicy::UntilResolved(
                        crate::features::notifications::NoticeSubject::Approval(
                            approval_id.clone(),
                        ),
                    ),
                    format!("approval requested for {tool_name}"),
                );
                if self.focus_manager.active_mode() != AppMode::Surface(SurfaceId::Approval) {
                    self.push_surface(SurfaceId::Approval);
                }
            }
            piko_protocol::ApprovalEvent::Resolved {
                session_id,
                approval_id,
                decision,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.approvals.resolve(&approval_id);
                self.notifications.resolve(
                    &crate::features::notifications::NoticeSubject::Approval(approval_id.clone()),
                );
                self.status = format!("approval {approval_id} resolved: {decision:?}");
                if self.approvals.is_empty()
                    && self.focus_manager.active_mode() == AppMode::Surface(SurfaceId::Approval)
                {
                    self.pop_focus();
                }
                if self.approvals.is_empty()
                    && !self.interactions.is_empty()
                    && self.focus_manager.active_mode()
                        != AppMode::Surface(SurfaceId::ToolInteraction)
                {
                    self.push_surface(SurfaceId::ToolInteraction);
                }
            }
        }
        effects
    }

    pub(super) fn apply_auth(&mut self, event: piko_protocol::AuthEvent) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            piko_protocol::AuthEvent::LoginBrowser {
                provider,
                authorization_url,
                ..
            } => {
                self.notifications.push_with(
                    NoticeScope::Global,
                    NotificationLevel::Warning,
                    NoticePolicy::UntilResolved(NoticeSubject::Auth(provider.clone())),
                    format!(
                        "{provider} login opened in your browser; if it did not open, use {authorization_url}"
                    ),
                );
                effects.push(Effect::open_url(authorization_url));
            }
            piko_protocol::AuthEvent::LoginDeviceCode {
                provider,
                user_code,
                verification_uri,
                ..
            } => {
                self.notifications.push_with(
                    NoticeScope::Global,
                    NotificationLevel::Warning,
                    NoticePolicy::UntilResolved(NoticeSubject::Auth(provider.clone())),
                    format!("{provider} login: open {verification_uri} and enter {user_code}"),
                );
            }
            piko_protocol::AuthEvent::LoginSuccess { provider, .. } => {
                self.notifications
                    .resolve(&NoticeSubject::Auth(provider.clone()));
                self.notify(
                    NotificationLevel::Info,
                    format!("{provider} login succeeded"),
                );
                effects.push(Effect::send(piko_protocol::Command::ModelList {
                    command_id: command_id(),
                }));
            }
            piko_protocol::AuthEvent::LoginFailed {
                provider, error, ..
            } => {
                self.notifications
                    .resolve(&NoticeSubject::Auth(provider.clone()));
                self.notify(
                    NotificationLevel::Error,
                    format!("{provider} login failed: {error}"),
                );
            }
            piko_protocol::AuthEvent::LoggedOut { provider } => {
                self.notifications
                    .resolve(&NoticeSubject::Auth(provider.clone()));
                self.notify(NotificationLevel::Info, format!("{provider} logged out"));
            }
        }
        effects
    }

    pub(super) fn apply_model(&mut self, event: piko_protocol::ModelEvent) -> Vec<Effect> {
        let effects = Vec::new();
        let piko_protocol::ModelEvent::ConfigChanged {
            model_id,
            provider,
            thinking_level,
            context_window,
            ..
        } = event;
        self.model.active_model_id = (!model_id.is_empty()).then(|| model_id.clone());
        self.model.active_provider = (!provider.is_empty()).then(|| provider.clone());
        self.model.active_thinking_level = Some(
            thinking_level
                .map(|level| level.as_str().to_string())
                .unwrap_or_else(|| "off".to_string()),
        );
        self.model.host_context_window = context_window.filter(|w| *w > 0);
        self.status =
            if self.model.active_model_id.is_some() && self.model.active_provider.is_some() {
                format!("model {provider}/{model_id}")
            } else {
                "no model active".to_string()
            };
        effects
    }

    pub(super) fn apply_usage(&mut self, event: piko_protocol::UsageEvent) -> Vec<Effect> {
        let effects = Vec::new();
        let piko_protocol::UsageEvent::Updated {
            session_id,
            used,
            size,
            cumulative,
            ..
        } = event;
        if !self.accepts_session(&session_id) {
            return effects;
        }
        if used > 0 {
            self.session.last_context_tokens = Some(used);
        }
        if let Some(window) = size.filter(|w| *w > 0) {
            self.model.host_context_window = Some(window);
        }
        if let Some(cumulative) = cumulative {
            self.session.cumulative_usage = Some(cumulative);
        }
        effects
    }
}
