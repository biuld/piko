use std::fs;
use std::path::Path;

use uuid::Uuid;

use super::super::session_store::SessionStore;
use super::super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};
use super::helpers::{copy_dir_all, timestamp};
use super::load::load_session_dir;

impl JsonlSessionRepository {
    pub fn fork(
        &self,
        _source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        if entry_id.is_some() {
            return Err(SessionStorageError::Invalid {
                path: source_dir.to_path_buf(),
                message: "branch-point fork is not yet supported by schema v3".into(),
            });
        }
        let source = SessionStore::new(source_dir);
        let source_manifest = source.load_manifest()?;
        let forked_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let cwd_dir = self.session_dir(&source_manifest.cwd);
        let forked_dir = cwd_dir.join(format!(
            "{}_{}",
            created_at.replace([':', '.'], "-"),
            forked_id
        ));
        source.fork_to(
            &forked_dir,
            forked_id,
            created_at.parse().unwrap_or_default(),
        )?;
        load_session_dir(&forked_dir)
    }

    pub fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError> {
        if !input_path.exists() {
            return Err(SessionStorageError::NotFound(
                input_path.to_string_lossy().to_string(),
            ));
        }
        if !input_path.is_dir() {
            return Err(SessionStorageError::Invalid {
                path: input_path.to_path_buf(),
                message: "import requires a session directory".into(),
            });
        }
        let src_session = load_session_dir(input_path)?;
        let dest_dir = self.session_dir(&src_session.state.cwd);
        fs::create_dir_all(&dest_dir).map_err(|e| SessionStorageError::Io {
            path: dest_dir.clone(),
            source: e,
        })?;
        let name = input_path.file_name().ok_or(SessionStorageError::Invalid {
            path: input_path.to_path_buf(),
            message: "missing name".into(),
        })?;
        let dest = dest_dir.join(name);
        if dest != input_path {
            copy_dir_all(input_path, &dest).map_err(|e| SessionStorageError::Io {
                path: dest.clone(),
                source: e,
            })?;
        }
        load_session_dir(&dest)
    }
}
