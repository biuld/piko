//! Scoped command bindings.
//!
//! This module is the semantic boundary between normalized terminal input and
//! the product action reducer. It owns rule compilation and diagnostics; it
//! deliberately contains no application mutations.

mod chord;
mod config;
mod context;
mod defaults;
mod diagnostics;
mod registry;
mod resolver;

#[cfg(test)]
mod registry_tests;

use crate::terminal::TerminalProfile;

#[cfg(test)]
pub use chord::Key;
pub use chord::KeyStroke;
pub use chord::Modifiers;
pub use config::{BindingRuleSetting, KeybindingSettings};
#[cfg(test)]
pub use context::ActiveScope;
pub use context::{
    BindingContext, Condition, ContextAtom, Propagation, ScopeKind, ScopeStack, TextSink,
    active_scope_stack,
};
pub use diagnostics::{BindingDiagnostic, DiagnosticSeverity, ResolutionTrace};
pub use resolver::Resolution;

/// Where a rule came from. Sources are diagnostic metadata only; precedence
/// is determined by the active scope and conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleSource {
    BuiltIn,
    HostSettings,
}

/// A validated semantic binding rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingRule {
    pub id: String,
    pub key: KeyStroke,
    pub command: crate::input::command::CommandId,
    pub scope: ScopeKind,
    pub conditions: Vec<Condition>,
    pub source: RuleSource,
    pub enabled: bool,
}

/// Effective bindings for one immutable terminal profile.
#[derive(Clone, Debug)]
pub struct BindingRegistry {
    pub(super) profile: TerminalProfile,
    pub(super) rules: Vec<BindingRule>,
    pub(super) diagnostics: Vec<BindingDiagnostic>,
}
