use super::*;
use std::path::{Component, Path};

impl EffectivePermissions {
    pub(super) fn root(root: &Path, cwd: &Path) -> Result<PathBuf, PolicyError> {
        let root = if root.is_absolute() {
            root.to_path_buf()
        } else {
            cwd.join(root)
        };
        if root.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(PolicyError::Invalid(format!(
                "policy path contains '..': {}",
                root.display()
            )));
        }
        Ok(root.canonicalize().unwrap_or(root))
    }

    pub(super) fn resolve_missing(candidate: &Path) -> Result<PathBuf, PolicyError> {
        let mut missing = Vec::new();
        let mut ancestor = candidate;
        while !ancestor.exists() {
            missing.push(
                ancestor
                    .file_name()
                    .ok_or_else(|| PolicyError::Denied(candidate.display().to_string()))?,
            );
            ancestor = ancestor
                .parent()
                .ok_or_else(|| PolicyError::Denied(candidate.display().to_string()))?;
        }
        let mut resolved = ancestor.canonicalize()?;
        for part in missing.into_iter().rev() {
            resolved.push(part);
        }
        Ok(resolved)
    }

    pub fn authorize(
        &self,
        cwd: &Path,
        input: &Path,
        access: Access,
        must_exist: bool,
    ) -> Result<PathBuf, PolicyError> {
        let cwd = cwd.canonicalize()?;
        let candidate = if input.is_absolute() {
            input.to_path_buf()
        } else {
            cwd.join(input)
        };
        let resolved = if must_exist || candidate.exists() {
            candidate.canonicalize()?
        } else {
            Self::resolve_missing(&candidate)?
        };
        if resolved
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(PolicyError::Denied(resolved.display().to_string()));
        }
        for root in &self.denied_read_roots {
            if resolved.starts_with(Self::root(root, &cwd)?) {
                return Err(PolicyError::Denied(resolved.display().to_string()));
            }
        }
        if matches!(access, Access::Write) {
            for root in &self.denied_write_roots {
                if resolved.starts_with(Self::root(root, &cwd)?) {
                    return Err(PolicyError::Denied(resolved.display().to_string()));
                }
            }
        }
        let roots = match access {
            Access::Read => self.read_roots.iter().chain(self.scratch_roots.iter()),
            Access::Write => self.write_roots.iter().chain(self.scratch_roots.iter()),
        };
        for root in roots {
            if resolved.starts_with(Self::root(root, &cwd)?) {
                return Ok(resolved);
            }
        }
        Err(PolicyError::Denied(resolved.display().to_string()))
    }

    /// Absolute writable roots resolved against `cwd`, using the same
    /// canonicalization as [`EffectivePermissions::authorize`] (F-12 safety assessment).
    ///
    /// The projection is the write boundary that `authorize` enforces; it
    /// lets approval-time callers decide whether a write target is fully
    /// constrained without touching the filesystem beyond canonicalization.
    pub fn writable_roots(&self, cwd: &Path) -> Vec<PathBuf> {
        self.write_roots
            .iter()
            .chain(self.scratch_roots.iter())
            .filter_map(|root| Self::root(root, cwd).ok())
            .collect()
    }

    pub fn readable_roots(&self, cwd: &Path) -> Vec<PathBuf> {
        self.read_roots
            .iter()
            .chain(self.scratch_roots.iter())
            .filter_map(|root| Self::root(root, cwd).ok())
            .collect()
    }

    pub fn scratch_roots(&self, cwd: &Path) -> Vec<PathBuf> {
        self.scratch_roots
            .iter()
            .filter_map(|root| Self::root(root, cwd).ok())
            .collect()
    }

    /// Re-resolve `input` and verify it still maps to the previously
    /// authorized `expected` path (TOCTOU guard for file writes).
    ///
    /// Between authorization and execution a concurrent process could swap a
    /// symlink so the same lexical input resolves elsewhere; re-checking with
    /// the caller's original input right before the write closes that window.
    pub fn verify_resolved(
        &self,
        cwd: &Path,
        input: &Path,
        access: Access,
        must_exist: bool,
        expected: &Path,
    ) -> Result<(), PolicyError> {
        let resolved = self.authorize(cwd, input, access, must_exist)?;
        if resolved != expected {
            return Err(PolicyError::Denied(format!(
                "path changed between authorization and execution: {}",
                input.display()
            )));
        }
        Ok(())
    }
}
