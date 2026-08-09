//! F-17 permission profiles: profile resolution and command-policy
//! evaluation.
//!
//! Profiles bundle file/network policy (materialized into the sandbox
//! policy by the orchd wiring) and command policy (materialized into the
//! approval gateway: allowed prefixes auto-accept one-shot, denied prefixes
//! fail closed). The logic here is pure and unit-tested; all settings come
//! from the already-merged `[permissions]` section.

use std::collections::HashMap;

use crate::domain::config::{PermissionProfileSettings, PermissionsSettings};

/// Built-in profile name; mirrors the restricted workspace default.
pub const DEFAULT_PROFILE_NAME: &str = "default";

/// A resolved permission profile: file/network rules for the sandbox policy
/// and command rules for the approval gateway.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionProfile {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub scratch_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
    pub allow_escalation: bool,
    /// Command prefixes that auto-accept on-request approvals one-shot.
    pub allowed_commands: Vec<String>,
    /// Command prefixes that fail closed with `permission_denied`.
    pub denied_commands: Vec<String>,
}

impl Default for PermissionProfile {
    fn default() -> Self {
        Self {
            read_roots: vec![".".into()],
            write_roots: vec![".".into()],
            scratch_roots: Vec::new(),
            deny_paths: vec![".piko".into()],
            allow_network: false,
            allow_escalation: true,
            allowed_commands: Vec::new(),
            denied_commands: Vec::new(),
        }
    }
}

impl From<&PermissionProfileSettings> for PermissionProfile {
    fn from(settings: &PermissionProfileSettings) -> Self {
        // Empty vectors inherit the permissive defaults per field, so a
        // profile that only declares command rules does not lock down file
        // or network access unexpectedly.
        let base = PermissionProfile::default();
        Self {
            read_roots: if settings.read_roots.is_empty() {
                base.read_roots
            } else {
                settings.read_roots.clone()
            },
            write_roots: if settings.write_roots.is_empty() {
                base.write_roots
            } else {
                settings.write_roots.clone()
            },
            scratch_roots: settings.scratch_roots.clone(),
            deny_paths: if settings.deny_paths.is_empty() {
                base.deny_paths
            } else {
                settings.deny_paths.clone()
            },
            allow_network: settings.allow_network,
            allow_escalation: settings.allow_escalation.unwrap_or(true),
            allowed_commands: settings.allowed_commands.clone(),
            denied_commands: settings.denied_commands.clone(),
        }
    }
}

/// Resolved command policy handed to the approval gateway.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionConfig {
    pub allowed_command_prefixes: Vec<String>,
    pub denied_command_prefixes: Vec<String>,
    pub allow_escalation: bool,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            allowed_command_prefixes: Vec::new(),
            denied_command_prefixes: Vec::new(),
            allow_escalation: true,
        }
    }
}

/// Materialized file/network rules for one agent role (F-19). Mirrors the
/// sandbox-facing fields of a permission profile; empty vectors inherit the
/// permissive defaults per field when materialized.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleSandboxPolicy {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub scratch_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
}

/// Fully resolved permissions for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPermissions {
    /// Whether a `[permissions]` section exists, so the profile's
    /// file/network rules materialize into the sandbox policy. Without a
    /// section, sandbox policy resolution is unchanged.
    pub materialize: bool,
    pub profile: PermissionProfile,
    pub config: PermissionConfig,
    /// F-19: role → command policy for the approval gateway. Absent roles
    /// use `config` (the session profile).
    pub role_configs: HashMap<String, PermissionConfig>,
    /// F-19: role → materialized file/network policy for the sandbox.
    /// Absent roles use the session policy.
    pub role_policies: HashMap<String, RoleSandboxPolicy>,
}

/// Resolve the active permission profile from merged settings.
///
/// - No `[permissions]` section: built-in `default`, no materialization.
/// - Built-in `default` (selected explicitly or by default): same as no
///   section and uses the built-in restricted workspace policy.
/// - Unknown profile name: warn and fall back to the built-in `default`.
/// - A user-defined profile: materializes file/network policy into the
///   sandbox policy and command rules into the approval gateway.
mod resolve;
#[cfg(test)]
mod tests;

pub use resolve::*;
