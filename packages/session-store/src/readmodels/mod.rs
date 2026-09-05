//! Durable CQRS projections published after each journal commit (F-37 / D-53).

mod catalog;
mod current;
mod files;
mod history;
#[cfg(test)]
mod history_tests;
mod open;
mod query;
mod trajectory;

use std::path::Path;

use self::files::atomic_json;
use crate::journal::{DurableCommit, SessionIdentityFile};
use crate::{Result, SessionAggregate};

pub use catalog::{CatalogView, inspect_catalog};
pub use files::READ_MODEL_SCHEMA;
pub use history::{
    HistoryCommit, HistoryEvent, HistoryProjection, HistoryProvenance, HistoryTransition,
    apply_commit as apply_history_commit,
};
pub(crate) use open::load_or_rebuild;
pub use query::{
    InspectionBundle, query_catalog, query_current, query_history, query_inspection,
    query_trajectory,
};
pub use trajectory::{
    TrajectoryProjection, TrajectoryRunProjection, apply_commit as apply_trajectory_commit,
};

pub(crate) fn ensure_dir(path: &Path) -> Result<()> {
    files::ensure_dir(path)
}

pub(crate) fn publish(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    aggregate: &SessionAggregate,
    trajectory: &TrajectoryProjection,
    history: &HistoryProjection,
    checksum: &str,
) -> Result<()> {
    files::ensure_dir(path)?;
    catalog::write(path, session_id, journal_generation, aggregate, checksum)?;
    current::write(path, session_id, journal_generation, aggregate, checksum)?;
    trajectory::write(path, session_id, journal_generation, trajectory, checksum)?;
    history::write(path, session_id, journal_generation, history, checksum)?;
    write_head(
        path,
        session_id,
        journal_generation,
        aggregate.revision,
        checksum,
    )
}

pub(crate) fn rebuild_from_commits(
    path: &Path,
    identity: &SessionIdentityFile,
    commits: &[DurableCommit],
) -> Result<(
    SessionAggregate,
    TrajectoryProjection,
    HistoryProjection,
    Option<String>,
)> {
    let mut aggregate = SessionAggregate::default();
    let mut trajectory = TrajectoryProjection::default();
    let mut history = HistoryProjection::default();
    for commit in commits {
        aggregate.apply_for_replay(commit)?;
        apply_trajectory_commit(&mut trajectory, commit);
        apply_history_commit(&mut history, commit);
    }
    let checksum = commits.last().map(|commit| commit.checksum.value.clone());
    if let Some(checksum) = checksum.as_deref() {
        publish(
            path,
            &identity.session_id,
            &identity.journal_generation,
            &aggregate,
            &trajectory,
            &history,
            checksum,
        )?;
    }
    Ok((aggregate, trajectory, history, checksum))
}

pub(crate) fn load_current_if_current(
    path: &Path,
    identity: &SessionIdentityFile,
    tip: Option<&DurableCommit>,
) -> Result<Option<(SessionAggregate, TrajectoryProjection, HistoryProjection)>> {
    let Some(current) = current::load(path)? else {
        return Ok(None);
    };
    let aligned = if let Some(tip) = tip {
        files::envelope_matches(
            current.schema_version,
            &current.session_id,
            &current.journal_generation,
            current.through_revision,
            &current.through_checksum,
            identity,
            tip,
        )
    } else if let Some(head) = load_head(path)? {
        current.schema_version == files::READ_MODEL_SCHEMA
            && current.session_id == identity.session_id
            && current.journal_generation == identity.journal_generation
            && current.through_revision == head.revision
            && current.through_checksum == head.checksum
    } else {
        false
    };
    if !aligned {
        return Ok(None);
    }
    let Some(trajectory) = trajectory::load(path)? else {
        return Ok(None);
    };
    let trajectory_aligned = if let Some(tip) = tip {
        files::envelope_matches(
            trajectory.schema_version,
            &trajectory.session_id,
            &trajectory.journal_generation,
            trajectory.through_revision,
            &trajectory.through_checksum,
            identity,
            tip,
        )
    } else {
        trajectory.schema_version == files::READ_MODEL_SCHEMA
            && trajectory.session_id == identity.session_id
            && trajectory.journal_generation == identity.journal_generation
            && trajectory.through_revision == current.through_revision
            && trajectory.through_checksum == current.through_checksum
    };
    if !trajectory_aligned {
        return Ok(None);
    }
    let Some(history) = history::load(path)? else {
        return Ok(None);
    };
    let history_aligned = if let Some(tip) = tip {
        files::envelope_matches(
            history.schema_version,
            &history.session_id,
            &history.journal_generation,
            history.through_revision,
            &history.through_checksum,
            identity,
            tip,
        )
    } else {
        history.schema_version == files::READ_MODEL_SCHEMA
            && history.session_id == identity.session_id
            && history.journal_generation == identity.journal_generation
            && history.through_revision == current.through_revision
            && history.through_checksum == current.through_checksum
    };
    if !history_aligned {
        return Ok(None);
    }
    let mut trajectory = trajectory.projection;
    trajectory.refresh_counts();
    Ok(Some((current.aggregate, trajectory, history.projection)))
}

fn write_head(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    revision: u64,
    checksum: &str,
) -> Result<()> {
    atomic_json(
        &files::head_path(path),
        &HeadFile {
            schema_version: READ_MODEL_SCHEMA,
            session_id: session_id.to_string(),
            journal_generation: journal_generation.to_string(),
            revision,
            checksum: checksum.to_string(),
        },
    )
}

pub(crate) fn load_head(path: &Path) -> Result<Option<HeadFile>> {
    files::load_json(&files::head_path(path))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadFile {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub revision: u64,
    pub checksum: String,
}
