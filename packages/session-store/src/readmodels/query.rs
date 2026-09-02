//! Ordinary session queries: read published models, rebuild only on lag.

use std::path::Path;

use crate::journal::{OpenOptions, SessionIdentityFile, SessionStore};
use crate::journal_queries::facts_from_aggregate;
use crate::replay::last_open_commit;
use crate::{Result, SessionAggregate, StoreError};

use super::catalog::CatalogView;
use super::files::READ_MODEL_SCHEMA;
use super::{current, trajectory};
use super::{inspect_catalog, load_head};

pub fn query_catalog(path: &Path) -> Result<CatalogView> {
    if let Some(view) = inspect_catalog(path)?
        && published_matches_tip(path, view.facts.revision)?
    {
        return Ok(view);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (aggregate, _) = opened.store.republish_readmodels()?;
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
    let (aggregate, _) = opened.store.republish_readmodels()?;
    Ok(aggregate)
}

pub fn query_trajectory(path: &Path) -> Result<crate::TrajectoryProjection> {
    if let Some(mut projection) = load_trajectory_if_published(path)? {
        projection.refresh_counts();
        return Ok(projection);
    }
    let opened = SessionStore::open(path, OpenOptions::default())?;
    let (_, trajectory) = opened.store.republish_readmodels()?;
    Ok(trajectory)
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
