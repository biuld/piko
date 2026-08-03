//! F-11 guardian review execution (application side).
//!
//! The approval gateway consumes review decisions; this module owns the
//! host-side review call: bounded transcript from the durable session tree,
//! reviewer model resolution with default-model fallback, and strict-JSON
//! parsing (fail closed).

use crate::application::host_app::HostApp;
use crate::domain::guardian::{
    DEFAULT_MAX_CHARS_PER_ENTRY, DEFAULT_MAX_ENTRIES, GuardianDecision, GuardianReviewRequest,
    build_review_context, parse_decision, run_review,
};

impl HostApp {
    /// Run the bounded guardian review for a tool approval request. Any
    /// failure (no session, no model executor, model error, malformed JSON)
    /// returns `Err` so the gateway fails the request closed.
    pub(crate) async fn run_guardian_review(
        &self,
        session_id: &str,
        request: &GuardianReviewRequest,
    ) -> Result<GuardianDecision, String> {
        let entries = {
            let state = self.state.lock().await;
            state
                .session(session_id)
                .map_err(|error| format!("no session {session_id} for guardian review: {error}"))?
                .entries
                .clone()
        };
        let context =
            build_review_context(&entries, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_CHARS_PER_ENTRY);
        if context.is_empty() {
            return Err("no review context available for guardian review".into());
        }

        let executor = self
            .model_executor
            .lock()
            .await
            .clone()
            .ok_or_else(|| "model executor unavailable for guardian review".to_string())?;

        let (model_id, provider) = {
            let settings = self.settings.lock().await;
            let guardian = settings.guardian.as_ref();
            let default_model = settings
                .default_model
                .clone()
                .unwrap_or_else(|| "default".into());
            let default_provider = settings
                .default_provider
                .clone()
                .unwrap_or_else(|| "default".into());
            (
                guardian
                    .and_then(|settings| settings.model.clone())
                    .unwrap_or(default_model),
                guardian
                    .and_then(|settings| settings.provider.clone())
                    .unwrap_or(default_provider),
            )
        };
        let model = piko_protocol::messages::Model {
            id: model_id.clone(),
            name: model_id,
            provider,
            base_url: None,
        };

        let text = run_review(
            executor,
            model,
            context,
            &request.tool_name,
            &request.tool_args,
        )
        .await?;
        parse_decision(&text)
    }
}
