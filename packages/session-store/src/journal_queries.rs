use std::path::Path;

use crate::journal::{DurableCommit, SessionStore, VerificationReport};
use crate::replay::read_all;
use crate::{Result, SessionAggregate, StoreError};

/// One raw journal event with its durable commit position. Observational
/// readers (for example the trajectory query) use this to replay optional
/// event types without affecting the acknowledged session projection.
#[derive(Debug, Clone, PartialEq)]
pub struct RawJournalEvent {
    pub revision: u64,
    pub committed_at: i64,
    pub event: crate::RawEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalFacts {
    pub revision: u64,
    pub name: Option<String>,
    pub updated_at: i64,
    pub message_count: u64,
    pub extra_tree_count: u64,
    pub first_user_message: Option<String>,
}

pub(crate) fn events_from_commits(commits: &[DurableCommit]) -> Vec<RawJournalEvent> {
    commits.iter().flat_map(events_from_commit).collect()
}

pub(crate) fn events_from_commit(
    commit: &DurableCommit,
) -> impl Iterator<Item = RawJournalEvent> + '_ {
    commit.events.iter().cloned().map(|event| RawJournalEvent {
        revision: commit.revision,
        committed_at: commit.committed_at,
        event,
    })
}

impl SessionStore {
    pub fn verify(&self) -> Result<VerificationReport> {
        let (commits, _, segments) = read_all(&self.inner.path, false)?;
        Ok(VerificationReport {
            revision: commits.last().map_or(0, |commit| commit.revision),
            segment_count: segments,
        })
    }

    pub fn revision(&self) -> u64 {
        self.inner
            .aggregate
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .revision
    }

    /// Lightweight aggregate facts for list summaries. Reads under the
    /// aggregate lock without cloning the full aggregate (which is large for
    /// big sessions).
    pub fn journal_facts(&self) -> JournalFacts {
        let aggregate = self
            .inner
            .aggregate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        facts_from_aggregate(&aggregate)
    }

    pub fn aggregate(&self) -> SessionAggregate {
        self.inner
            .aggregate
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.inner.session_id
    }

    pub(crate) fn journal_generation(&self) -> &str {
        &self.inner.journal_generation
    }

    pub(crate) fn commit_at(&self, revision: u64) -> Result<DurableCommit> {
        read_all(&self.inner.path, false)?
            .0
            .into_iter()
            .find(|commit| commit.revision == revision)
            .ok_or_else(|| StoreError::InvalidEvent(format!("missing commit revision {revision}")))
    }

    /// All raw journal events in commit order, including optional
    /// (`ignorable`) event types that the acknowledged projection skips.
    /// Served from the in-memory cache filled on open and append.
    pub fn raw_events(&self) -> Result<Vec<RawJournalEvent>> {
        Ok(self
            .inner
            .raw_events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone())
    }

    /// Raw events with `revision > after_revision`. Used by observational
    /// readers to refresh incrementally after the first full decode.
    pub fn raw_events_after(&self, after_revision: u64) -> Result<Vec<RawJournalEvent>> {
        Ok(self
            .inner
            .raw_events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|event| event.revision > after_revision)
            .cloned()
            .collect())
    }
}

pub(crate) fn facts_from_aggregate(aggregate: &SessionAggregate) -> JournalFacts {
    let mut messages = aggregate.messages.values().collect::<Vec<_>>();
    messages.sort_by_key(|message| message.revision);
    let first_user_message = messages.into_iter().find_map(|message| {
        let piko_protocol::Message::User { content, .. } = &message.data.message else {
            return None;
        };
        match content {
            piko_protocol::MessageContent::String(text) if !text.trim().is_empty() => {
                Some(text.clone())
            }
            piko_protocol::MessageContent::Blocks(blocks) => {
                let text = blocks
                    .iter()
                    .filter_map(|block| match block {
                        piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            _ => None,
        }
    });
    JournalFacts {
        revision: aggregate.revision,
        name: aggregate.name.clone(),
        updated_at: aggregate.updated_at,
        message_count: aggregate.messages.len() as u64,
        extra_tree_count: aggregate
            .tree_entries
            .values()
            .filter(|entry| !aggregate.messages.contains_key(&entry.data.entry_id))
            .count() as u64,
        first_user_message,
    }
}
