use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePermissions {
    pub version: u32,
    pub read_roots: Vec<PathBuf>,
    pub write_roots: Vec<PathBuf>,
    pub scratch_roots: Vec<PathBuf>,
    pub denied_read_roots: Vec<PathBuf>,
    /// Paths that may be read but never modified by restricted operations.
    pub denied_write_roots: Vec<PathBuf>,
    pub network: NetworkPermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPermissions {
    Restricted,
    Enabled,
}

impl NetworkPermissions {
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl From<bool> for NetworkPermissions {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Restricted
        }
    }
}

#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("invalid policy: {0}")]
    Invalid(String),
    #[error("access denied: {0}")]
    Denied(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

mod evaluate;
#[cfg(test)]
mod tests;
