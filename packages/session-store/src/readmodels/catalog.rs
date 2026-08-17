use std::path::Path;

use serde::{Deserialize, Serialize};

use super::files::atomic_json;
use crate::journal::{SessionDescriptor, SessionIdentityFile};
use crate::journal_queries::{JournalFacts, facts_from_aggregate};
use crate::{Result, SCHEMA_VERSION, SessionAggregate, StoreError};

use super::files::{self, READ_MODEL_SCHEMA};
use super::load_head;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub through_revision: u64,
    pub through_checksum: String,
    pub name: Option<String>,
    pub updated_at: i64,
    pub message_count: u64,
    pub extra_tree_count: u64,
    pub first_user_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogView {
    pub identity: SessionDescriptor,
    pub facts: JournalFacts,
}

pub fn inspect_catalog(path: &Path) -> Result<Option<CatalogView>> {
    let path = path
        .canonicalize()
        .map_err(|source| crate::error::io_error(path, source))?;
    let identity_path = path.join("session.json");
    let data = std::fs::read(&identity_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            StoreError::NotFound(path.clone())
        } else {
            crate::error::io_error(&identity_path, source)
        }
    })?;
    let identity: SessionIdentityFile =
        serde_json::from_slice(&data).map_err(|source| StoreError::Json {
            path: identity_path,
            source,
        })?;
    if identity.schema_version != SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema {
            found: identity.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    let Some(head) = load_head(&path)? else {
        return Ok(None);
    };
    let Some(catalog) = load(&path)? else {
        return Ok(None);
    };
    if catalog.schema_version != READ_MODEL_SCHEMA
        || catalog.session_id != identity.session_id
        || catalog.journal_generation != identity.journal_generation
        || head.session_id != identity.session_id
        || head.journal_generation != identity.journal_generation
        || catalog.through_revision != head.revision
        || catalog.through_checksum != head.checksum
    {
        return Ok(None);
    }
    Ok(Some(CatalogView {
        identity: SessionDescriptor {
            session_id: identity.session_id,
            cwd: identity.cwd,
            created_at: identity.created_at,
        },
        facts: JournalFacts {
            revision: catalog.through_revision,
            name: catalog.name,
            updated_at: catalog.updated_at,
            message_count: catalog.message_count,
            extra_tree_count: catalog.extra_tree_count,
            first_user_message: catalog.first_user_message,
        },
    }))
}

pub(crate) fn write(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    aggregate: &SessionAggregate,
    checksum: &str,
) -> Result<()> {
    let facts = facts_from_aggregate(aggregate);
    atomic_json(
        &files::catalog_path(path),
        &CatalogFile {
            schema_version: READ_MODEL_SCHEMA,
            session_id: session_id.to_string(),
            journal_generation: journal_generation.to_string(),
            through_revision: aggregate.revision,
            through_checksum: checksum.to_string(),
            name: facts.name,
            updated_at: facts.updated_at,
            message_count: facts.message_count,
            extra_tree_count: facts.extra_tree_count,
            first_user_message: facts.first_user_message,
        },
    )
}

fn load(path: &Path) -> Result<Option<CatalogFile>> {
    files::load_json(&files::catalog_path(path))
}
