use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::domain::compaction::active_branch_entries;

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
        match entry_id {
            None => self.fork_full(source_dir),
            Some(entry_id) => self.fork_at_entry(source_dir, entry_id),
        }
    }

    fn allocate_fork_dir(
        &self,
        cwd: &str,
        forked_id: &str,
        created_at: &str,
    ) -> std::path::PathBuf {
        let cwd_dir = self.session_dir(cwd);
        cwd_dir.join(format!(
            "{}_{}",
            created_at.replace([':', '.'], "-"),
            forked_id
        ))
    }

    fn fork_full(&self, source_dir: &Path) -> Result<PersistedSession, SessionStorageError> {
        let source = SessionStore::new(source_dir);
        let source_projection = source.load_projection()?;
        let forked_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let forked_dir = self.allocate_fork_dir(&source_projection.cwd, &forked_id, &created_at);
        source.fork_to(
            &forked_dir,
            forked_id,
            created_at.parse().unwrap_or_default(),
        )?;
        load_session_dir(&forked_dir)
    }

    fn fork_at_entry(
        &self,
        source_dir: &Path,
        entry_id: &str,
    ) -> Result<PersistedSession, SessionStorageError> {
        // Validate against the same projected entry set clients see after open.
        let source_projected = load_session_dir(source_dir)?;
        let has_entry = source_projected
            .state
            .entries
            .iter()
            .any(|entry| entry.id() == entry_id);
        if !has_entry {
            return Err(SessionStorageError::Invalid {
                path: source_dir.to_path_buf(),
                message: format!("unknown tree entry: {entry_id}"),
            });
        }
        let retained = active_branch_entries(&source_projected.state.entries, Some(entry_id));
        if retained.is_empty() {
            return Err(SessionStorageError::Invalid {
                path: source_dir.to_path_buf(),
                message: format!("unknown tree entry: {entry_id}"),
            });
        }

        let source = SessionStore::new(source_dir);
        let source_projection = source.load_projection()?;
        let forked_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let forked_dir = self.allocate_fork_dir(&source_projection.cwd, &forked_id, &created_at);

        let write_result = source.fork_to_at_entry(
            &forked_dir,
            forked_id,
            created_at.parse().unwrap_or_default(),
            entry_id,
            &retained,
        );
        if write_result.is_err() {
            let _ = fs::remove_dir_all(&forked_dir);
        }
        write_result?;
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
        let source = SessionStore::new(input_path);
        source.with_io(|| {
            // Loading through `source` populates and retains its Journal
            // handle, so the filesystem writer lock remains held throughout
            // the copy as well as the in-process session IO lock.
            source.load_projection()?;
            let src_session = load_session_dir(input_path)?;
            let dest_dir = self.session_dir(&src_session.state.cwd);
            fs::create_dir_all(&dest_dir).map_err(|source| SessionStorageError::Io {
                path: dest_dir.clone(),
                source,
            })?;
            let name = input_path.file_name().ok_or(SessionStorageError::Invalid {
                path: input_path.to_path_buf(),
                message: "missing name".into(),
            })?;
            let dest = dest_dir.join(name);
            if dest == input_path {
                return Ok(src_session);
            }
            if dest.exists() {
                return Err(SessionStorageError::Invalid {
                    path: dest,
                    message: "import destination already exists".into(),
                });
            }

            let staging_root = dest_dir.join(".staging");
            fs::create_dir_all(&staging_root).map_err(|source| SessionStorageError::Io {
                path: staging_root.clone(),
                source,
            })?;
            let staging = staging_root.join(format!(
                "{}-{}",
                name.to_string_lossy(),
                uuid::Uuid::new_v4()
            ));
            let result = (|| {
                copy_dir_all(input_path, &staging).map_err(|source| SessionStorageError::Io {
                    path: staging.clone(),
                    source,
                })?;
                // Validate the complete copied journal before publication.
                load_session_dir(&staging)?;
                fs::rename(&staging, &dest).map_err(|source| SessionStorageError::Io {
                    path: dest.clone(),
                    source,
                })?;
                fs::File::open(&staging_root)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|source| SessionStorageError::Io {
                        path: staging_root.clone(),
                        source,
                    })?;
                fs::File::open(&dest_dir)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|source| SessionStorageError::Io {
                        path: dest_dir.clone(),
                        source,
                    })?;
                load_session_dir(&dest)
            })();
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            result
        })
    }
}
