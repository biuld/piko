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
            | Command::SessionHistoryWorkPageGet { session_id, .. }
            | Command::SessionHistoryJournalPageGet { session_id, .. }
            | Command::SessionHistoryTranscriptPageGet { session_id, .. }
            | Command::SessionHistoryItemGet { session_id, .. } => session_id.clone(),
            _ => unreachable!("history command routing"),
        };
        let query = crate::application::SessionHistoryQuery::new(
            self.session_paths.clone(),
            self.session_store_factory.clone(),
            self.storage.clone(),
        );
        let result = match command {
            Command::SessionHistoryOverviewGet {
                after_cursor,
                limit,
                ..
            } => query
                .overview(&session_id, after_cursor.as_deref(), limit)
                .await
                .map(|overview| CommandResult::SessionHistoryOverviewGot {
                    overview,
                    timestamp: now_ms(),
                }),
            Command::SessionHistoryWorkPageGet {
                root_input_id,
                expected_revision,
                after_cursor,
                limit,
                ..
            } => query
                .work_page(
                    &session_id,
                    &root_input_id,
                    expected_revision,
                    after_cursor.as_deref(),
                    limit,
                )
                .await
                .map(|page| CommandResult::SessionHistoryWorkPaged {
                    page,
                    timestamp: now_ms(),
                }),
            Command::SessionHistoryJournalPageGet {
                expected_revision,
                after_cursor,
                limit,
                provenance,
                ..
            } => query
                .journal_page(
                    &session_id,
                    expected_revision,
                    after_cursor.as_deref(),
                    limit,
                    provenance,
                )
                .await
                .map(|page| CommandResult::SessionHistoryJournalPaged {
                    page,
                    timestamp: now_ms(),
                }),
            Command::SessionHistoryTranscriptPageGet {
                expected_revision,
                after_cursor,
                limit,
                ..
            } => query
                .transcript_page(
                    &session_id,
                    expected_revision,
                    after_cursor.as_deref(),
                    limit,
                )
                .await
                .map(|page| CommandResult::SessionHistoryTranscriptPaged {
                    page,
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
