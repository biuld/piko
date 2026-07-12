use std::fs;
use std::path::{Path, PathBuf};

use super::super::SessionStorageError;

pub(super) fn encode_cwd(cwd: &str) -> String {
    format!(
        "cwd_{}",
        cwd.trim_start_matches(['/', '\\'])
            .replace(['/', '\\', ':'], "-")
    )
}

pub(super) fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

pub(super) fn commit_storage_error(error: piko_protocol::CommitError) -> SessionStorageError {
    SessionStorageError::Invalid {
        path: PathBuf::from("agent shard"),
        message: error.to_string(),
    }
}

pub(super) fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let t = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &t)?;
        } else {
            fs::copy(e.path(), &t)?;
        }
    }
    Ok(())
}
