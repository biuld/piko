use super::*;
use crate::api::CommandResult;

impl HostServer {
    pub(super) async fn apply_history_command(
        &self,
        command: Command,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let command_id = command.command_id().to_string();
        let session_id = match &command {
            Command::SessionHistoryOverviewGet { session_id, .. }
            | Command::SessionHistoryAgentStreamGet { session_id, .. }
            | Command::SessionHistoryLaneGet { session_id, .. }
            | Command::SessionHistoryItemGet { session_id, .. } => session_id.clone(),
            _ => unreachable!("history command routing"),
        };
        let query = crate::application::SessionHistoryQuery::new(
            self.session_paths.clone(),
            self.session_store_factory.clone(),
            self.storage.clone(),
            self.history_cache.clone(),
        );
        let result = match command {
            Command::SessionHistoryOverviewGet { .. } => {
                query.overview(&session_id).await.map(|overview| {
                    CommandResult::SessionHistoryOverviewGot {
                        overview,
                        timestamp: now_ms(),
                    }
                })
            }
            Command::SessionHistoryAgentStreamGet {
                agent_instance_id,
                expected_revision,
                after_cursor,
                limit,
                ..
            } => query
                .agent_stream(
                    &session_id,
                    &agent_instance_id,
                    expected_revision,
                    after_cursor.as_deref(),
                    limit,
                )
                .await
                .map(|page| CommandResult::SessionHistoryAgentStreamPaged {
                    page,
                    timestamp: now_ms(),
                }),
            Command::SessionHistoryLaneGet {
                agent_instance_id,
                expected_revision,
                ..
            } => query
                .lane_summary(&session_id, &agent_instance_id, expected_revision)
                .await
                .map(|summary| CommandResult::SessionHistoryLaneGot {
                    summary,
                    timestamp: now_ms(),
                }),
            Command::SessionHistoryItemGet { item_ref, .. } => query
                .item_detail(&session_id, &item_ref)
                .await
                .map(|detail| CommandResult::SessionHistoryItemGot {
                    detail,
                    timestamp: now_ms(),
                }),
            _ => unreachable!("history command routing"),
        };
        let result = match result {
            Err(ProtocolError::HistoryRevisionChanged { current_revision }) => {
                CommandResult::HistoryRevisionChanged {
                    session_id,
                    current_revision,
                }
            }
            other => other?,
        };
        Ok(vec![ServerMessage::CommandResponse {
            command_id,
            result: Ok(result),
        }])
    }
}
