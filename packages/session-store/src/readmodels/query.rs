//! Ordinary session queries: read published models, rebuild only on lag.

use std::path::Path;

use crate::journal::{OpenOptions, SessionIdentityFile, SessionStore};
use crate::journal_queries::facts_from_aggregate;
use crate::replay::last_open_commit;
use crate::{HistoryProjection, Result, SessionAggregate, StoreError, TrajectoryProjection};

use super::catalog::CatalogView;
use super::files::READ_MODEL_SCHEMA;
use super::{current, history, trajectory};
use super::{inspect_catalog, load_head};

#[derive(Debug, Clone, PartialEq)]
pub struct InspectionBundle {
    pub revision: u64,
    pub checksum: String,
    pub current: SessionAggregate,
    pub history: HistoryProjection,
    pub trajectory: TrajectoryProjection,
}

pub fn query_catalog(path: &Path) -> Result<CatalogView> {
    if let Some(view) = inspect_catalog(path)?
        && published_matches_tip(path, view.facts.revision)?
    {
        return Ok(view);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (aggregate, _, _) = opened.store.republish_readmodels()?;
    Ok(CatalogView {
        identity: SessionStore::inspect(path)?,
        facts: facts_from_aggregate(&aggregate),
    })
}

pub fn query_current(path: &Path) -> Result<SessionAggregate> {
    if let Some(aggregate) = load_current_if_published(path)? {
        return Ok(aggregate);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (aggregate, _, _) = opened.store.republish_readmodels()?;
    Ok(aggregate)
}

pub fn query_trajectory(path: &Path) -> Result<crate::TrajectoryProjection> {
    if let Some(mut projection) = load_trajectory_if_published(path)? {
        projection.refresh_counts();
        return Ok(projection);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (_, trajectory, _) = opened.store.republish_readmodels()?;
    Ok(trajectory)
}

pub fn query_history(path: &Path) -> Result<HistoryProjection> {
    if let Some(projection) = load_history_if_published(path)? {
        return Ok(projection);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (_, _, history) = opened.store.republish_readmodels()?;
    Ok(history)
}

/// Load all inspector inputs at one published watermark.
pub fn query_inspection(path: &Path) -> Result<InspectionBundle> {
    const MAX_ATTEMPTS: usize = 3;
    for _ in 0..MAX_ATTEMPTS {
        if let Some(bundle) = load_inspection_if_published(path)? {
            return Ok(bundle);
        }
    }
    // Repair once after transient publication races have had a chance to settle.
    let opened = SessionStore::open(path, OpenOptions::default())?;
    opened.store.republish_readmodels()?;
    for _ in 0..MAX_ATTEMPTS {
        if let Some(bundle) = load_inspection_if_published(path)? {
            return Ok(bundle);
        }
    }
    Err(StoreError::InspectionBusy)
}

fn load_identity(path: &Path) -> Result<SessionIdentityFile> {
    let identity_path = path.join("session.json");
    let data = std::fs::read(&identity_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            StoreError::NotFound(path.to_path_buf())
        } else {
            crate::error::io_error(&identity_path, source)
        }
    })?;
    let identity: SessionIdentityFile =
        serde_json::from_slice(&data).map_err(|source| StoreError::Json {
            path: identity_path,
            source,
        })?;
    if identity.schema_version != crate::SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema {
            found: identity.schema_version,
            supported: crate::SCHEMA_VERSION,
        });
    }
    Ok(identity)
}

fn published_matches_tip(path: &Path, through_revision: u64) -> Result<bool> {
    let Some(head) = load_head(path)? else {
        return Ok(false);
    };
    if head.revision != through_revision {
        return Ok(false);
    }
    let (tip, _) = last_open_commit(path, false)?;
    match tip {
        None => Ok(true),
        Some(tip) => Ok(tip.revision == head.revision && tip.checksum.value == head.checksum),
    }
}

fn load_current_if_published(path: &Path) -> Result<Option<SessionAggregate>> {
    let identity = load_identity(path)?;
    let Some(current) = current::load(path)? else {
        return Ok(None);
    };
    if current.schema_version != READ_MODEL_SCHEMA
        || current.session_id != identity.session_id
        || current.journal_generation != identity.journal_generation
        || !published_matches_tip(path, current.through_revision)?
    {
        return Ok(None);
    }
    let Some(head) = load_head(path)? else {
        return Ok(None);
    };
    if current.through_checksum != head.checksum {
        return Ok(None);
    }
    Ok(Some(current.aggregate))
}

fn load_trajectory_if_published(path: &Path) -> Result<Option<crate::TrajectoryProjection>> {
    let identity = load_identity(path)?;
    let Some(file) = trajectory::load(path)? else {
        return Ok(None);
    };
    if file.schema_version != READ_MODEL_SCHEMA
        || file.session_id != identity.session_id
        || file.journal_generation != identity.journal_generation
        || !published_matches_tip(path, file.through_revision)?
    {
        return Ok(None);
    }
    let Some(head) = load_head(path)? else {
        return Ok(None);
    };
    if file.through_checksum != head.checksum {
        return Ok(None);
    }
    Ok(Some(file.projection))
}

fn load_history_if_published(path: &Path) -> Result<Option<HistoryProjection>> {
    let identity = load_identity(path)?;
    let Some(file) = history::load(path)? else {
        return Ok(None);
    };
    if file.schema_version != READ_MODEL_SCHEMA
        || file.session_id != identity.session_id
        || file.journal_generation != identity.journal_generation
        || !published_matches_tip(path, file.through_revision)?
    {
        return Ok(None);
    }
    let Some(head) = load_head(path)? else {
        return Ok(None);
    };
    if file.through_checksum != head.checksum {
        return Ok(None);
    }
    Ok(Some(file.projection))
}

fn load_inspection_if_published(path: &Path) -> Result<Option<InspectionBundle>> {
    let identity = load_identity(path)?;
    let Some(head_before) = load_head(path)? else {
        return Ok(None);
    };
    if head_before.schema_version != READ_MODEL_SCHEMA
        || head_before.session_id != identity.session_id
        || head_before.journal_generation != identity.journal_generation
    {
        return Ok(None);
    }
    let (Some(current), Some(history), Some(trajectory)) = (
        current::load(path)?,
        history::load(path)?,
        trajectory::load(path)?,
    ) else {
        return Ok(None);
    };
    let aligned = [
        (
            current.schema_version,
            current.session_id.as_str(),
            current.journal_generation.as_str(),
            current.through_revision,
            current.through_checksum.as_str(),
        ),
        (
            history.schema_version,
            history.session_id.as_str(),
            history.journal_generation.as_str(),
            history.through_revision,
            history.through_checksum.as_str(),
        ),
        (
            trajectory.schema_version,
            trajectory.session_id.as_str(),
            trajectory.journal_generation.as_str(),
            trajectory.through_revision,
            trajectory.through_checksum.as_str(),
        ),
    ]
    .into_iter()
    .all(|(schema, session, generation, revision, checksum)| {
        schema == READ_MODEL_SCHEMA
            && session == identity.session_id
            && generation == identity.journal_generation
            && revision == head_before.revision
            && checksum == head_before.checksum
    });
    if !aligned || load_head(path)?.as_ref() != Some(&head_before) {
        return Ok(None);
    }
    let mut trajectory_projection = trajectory.projection;
    trajectory_projection.refresh_counts();
    Ok(Some(InspectionBundle {
        revision: head_before.revision,
        checksum: head_before.checksum,
        current: current.aggregate,
        history: history.projection,
        trajectory: trajectory_projection,
    }))
}
