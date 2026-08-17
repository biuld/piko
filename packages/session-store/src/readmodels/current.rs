use std::path::Path;

use serde::{Deserialize, Serialize};

use super::files::atomic_json;
use crate::{Result, SessionAggregate};

use super::files::{self, READ_MODEL_SCHEMA};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentFile {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub through_revision: u64,
    pub through_checksum: String,
    pub aggregate: SessionAggregate,
}

pub(crate) fn write(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    aggregate: &SessionAggregate,
    checksum: &str,
) -> Result<()> {
    atomic_json(
        &files::current_path(path),
        &CurrentFile {
            schema_version: READ_MODEL_SCHEMA,
            session_id: session_id.to_string(),
            journal_generation: journal_generation.to_string(),
            through_revision: aggregate.revision,
            through_checksum: checksum.to_string(),
            aggregate: aggregate.clone(),
        },
    )
}

pub(crate) fn load(path: &Path) -> Result<Option<CurrentFile>> {
    files::load_json(&files::current_path(path))
}
