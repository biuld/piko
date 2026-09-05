use piko_protocol::Command;

use super::{AppState, SurfaceId, command::SurfaceAction, command_id, effect::Effect};

impl AppState {
    pub(super) fn history_request(&mut self, mut command: Command) -> Vec<Effect> {
        // Request ownership survives panel teardown, including generic SessionList replies.
        match &mut command {
            Command::SessionList { command_id, .. }
            | Command::SessionHistoryOverviewGet { command_id, .. }
            | Command::SessionHistoryWorkPageGet { command_id, .. }
            | Command::SessionHistoryJournalPageGet { command_id, .. }
            | Command::SessionHistoryTranscriptPageGet { command_id, .. }
            | Command::SessionHistoryItemGet { command_id, .. } => {
                command_id.insert_str(0, "history:")
            }
            _ => unreachable!("history query command"),
        }
        self.history.pending_command_id = Some(command.command_id().to_string());
        self.history.loading = true;
        self.history.error = None;
        vec![Effect::send(command)]
    }

    pub(super) fn open_history(&mut self, requested: Option<String>) -> Vec<Effect> {
        let Some(session_id) = requested.or_else(|| self.session.id.clone()) else {
            return self.choose_history_session();
        };
        self.history.begin(session_id.clone());
        self.push_surface(SurfaceId::History);
        self.status = format!("loading history for {session_id}");
        self.history_request(Command::SessionHistoryOverviewGet {
            command_id: command_id(),
            session_id,
            after_cursor: None,
            limit: Some(100),
        })
    }

    pub(super) fn cycle_history_lens(&mut self, action: SurfaceAction) -> Vec<Effect> {
        if self.history.choosing_session {
            return Vec::new();
        }
        let backwards = matches!(action, SurfaceAction::HistoryLensPrevious);
        let lens = self.history.cycle_lens(backwards);
        self.history_fetch_lens(lens)
    }

    pub(super) fn select_history_lens(&mut self, index: usize) -> Vec<Effect> {
        if self.history.choosing_session {
            return Vec::new();
        }
        let lens = self.history.select_lens(index);
        self.history_fetch_lens(lens)
    }

    fn history_fetch_lens(&mut self, lens: crate::features::history::HistoryLens) -> Vec<Effect> {
        use crate::features::history::HistoryLens;
        let (Some(session_id), Some(overview)) = (
            self.history.session_id.clone(),
            self.history.overview.as_ref(),
        ) else {
            return Vec::new();
        };
        match lens {
            HistoryLens::Journal if self.history.journal.is_none() => {
                self.history_request(Command::SessionHistoryJournalPageGet {
                    command_id: command_id(),
                    session_id,
                    expected_revision: overview.revision,
                    after_cursor: None,
                    limit: Some(100),
                    provenance: self.history.provenance,
                })
            }
            HistoryLens::Transcript if self.history.transcript.is_none() => {
                self.history_request(Command::SessionHistoryTranscriptPageGet {
                    command_id: command_id(),
                    session_id,
                    expected_revision: overview.revision,
                    after_cursor: None,
                    limit: Some(100),
                })
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn refetch_history_journal(&mut self) -> Vec<Effect> {
        self.history.journal = None;
        if self.history.lens == crate::features::history::HistoryLens::Journal {
            self.history_fetch_lens(crate::features::history::HistoryLens::Journal)
        } else {
            Vec::new()
        }
    }

    pub(super) fn choose_history_session(&mut self) -> Vec<Effect> {
        self.history = Default::default();
        self.history.choosing_session = true;
        self.push_surface(SurfaceId::History);
        self.history_request(Command::SessionList {
            command_id: command_id(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
    }

    pub(super) fn history_next_page(&mut self) -> Vec<Effect> {
        use crate::features::history::HistoryLens;
        if self.history.loading
            || self.history.choosing_session
            || self.history.shows_detail_only()
            || self.history.selected.saturating_add(3) < self.history.row_count()
        {
            return Vec::new();
        }
        let Some(overview) = &self.history.overview else {
            return Vec::new();
        };
        let session_id = overview.session_id.clone();
        let expected_revision = overview.revision;
        let command_id = command_id();
        let command = match self.history.lens {
            HistoryLens::Work if self.history.work.is_some() => {
                let page = self.history.work.as_ref().unwrap();
                let Some(cursor) = page.next_cursor.clone() else {
                    return Vec::new();
                };
                Command::SessionHistoryWorkPageGet {
                    command_id,
                    session_id,
                    expected_revision,
                    root_input_id: page.root_input_id.clone(),
                    after_cursor: Some(cursor),
                    limit: Some(100),
                }
            }
            HistoryLens::Work => {
                let Some(cursor) = overview.next_cursor.clone() else {
                    return Vec::new();
                };
                Command::SessionHistoryOverviewGet {
                    command_id,
                    session_id,
                    after_cursor: Some(cursor),
                    limit: Some(100),
                }
            }
            HistoryLens::Journal => {
                let Some(cursor) = self
                    .history
                    .journal
                    .as_ref()
                    .and_then(|page| page.next_cursor.clone())
                else {
                    return Vec::new();
                };
                Command::SessionHistoryJournalPageGet {
                    command_id,
                    session_id,
                    expected_revision,
                    after_cursor: Some(cursor),
                    limit: Some(100),
                    provenance: self.history.provenance,
                }
            }
            HistoryLens::Transcript => {
                let Some(cursor) = self
                    .history
                    .transcript
                    .as_ref()
                    .and_then(|page| page.next_cursor.clone())
                else {
                    return Vec::new();
                };
                Command::SessionHistoryTranscriptPageGet {
                    command_id,
                    session_id,
                    expected_revision,
                    after_cursor: Some(cursor),
                    limit: Some(100),
                }
            }
            _ => return Vec::new(),
        };
        self.history_request(command)
    }
}
