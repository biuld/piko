use std::path::Path;

use crate::journal::{OpenOptions, RecoveryReport, SessionIdentityFile};
use crate::replay::{last_open_commit, read_all};
use crate::segments::normalize_segment_boundary;
use crate::{Result, SessionAggregate, StoreError, TrajectoryProjection};

use super::{load_current_if_current, rebuild_from_commits};

pub(crate) fn load_or_rebuild(
    path: &Path,
    identity: &SessionIdentityFile,
    options: OpenOptions,
    allow_empty_genesis: bool,
) -> Result<(
    SessionAggregate,
    TrajectoryProjection,
    Option<String>,
    RecoveryReport,
)> {
    let (tip, mut recovery) = last_open_commit(path, options.repair_incomplete_tail)?;
    if let Some(tip) = tip.as_ref()
        && (tip.session_id != identity.session_id
            || tip.journal_generation != identity.journal_generation)
    {
        return Err(StoreError::InvalidEvent(
            "journal identity/generation mismatch".into(),
        ));
    }
    // Projections may serve queries only when anchored to a durable journal
    // commit. An empty journal (no tip) must reach replay, which enforces
    // the genesis contract; read models have no independent authority.
    let fast = if tip.is_some() {
        load_current_if_current(path, identity, tip.as_ref())?
    } else {
        None
    };
    if let Some((mut aggregate, trajectory)) = fast {
        // `agent_work` is an in-memory shadow and is intentionally omitted
        // from the published JSON. Rebuild it before exposing an opened
        // session so fast-path loads have the same value as journal replay.
        aggregate.rebuild_work_projection();
        if normalize_segment_boundary(path, aggregate.revision)? {
            recovery.repaired = true;
        }
        let checksum = tip.map(|commit| commit.checksum.value);
        return Ok((aggregate, trajectory, checksum, recovery));
    }
    let (commits, replay_recovery, _) = read_all(path, options.repair_incomplete_tail)?;
    recovery.repaired |= replay_recovery.repaired;
    recovery.truncated_bytes += replay_recovery.truncated_bytes;
    recovery.last_verified_revision = replay_recovery.last_verified_revision;
    if commits.is_empty() && !allow_empty_genesis {
        return Err(StoreError::InvalidEvent(
            "journal is missing the session_created genesis commit".into(),
        ));
    }
    if commits.iter().any(|commit| {
        commit.session_id != identity.session_id
            || commit.journal_generation != identity.journal_generation
    }) {
        return Err(StoreError::InvalidEvent(
            "journal identity/generation mismatch".into(),
        ));
    }
    let (mut aggregate, trajectory, checksum) = rebuild_from_commits(path, identity, &commits)?;
    aggregate.rebuild_work_projection();
    if aggregate.revision > 0
        && (aggregate.session_id.as_deref() != Some(identity.session_id.as_str())
            || aggregate.cwd.as_deref() != Some(identity.cwd.as_str()))
    {
        return Err(StoreError::InvalidEvent(
            "journal aggregate does not match session identity".into(),
        ));
    }
    if normalize_segment_boundary(path, aggregate.revision)? {
        recovery.repaired = true;
    }
    Ok((aggregate, trajectory, checksum, recovery))
}
