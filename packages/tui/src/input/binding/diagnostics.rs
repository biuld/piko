use super::{BindingRule, KeyStroke, ScopeKind};
use crate::input::command::CommandId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingDiagnostic {
    pub severity: DiagnosticSeverity,
    pub rule_id: Option<String>,
    pub message: String,
}

impl BindingDiagnostic {
    pub fn error(rule_id: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            rule_id: rule_id.map(str::to_string),
            message: message.into(),
        }
    }

    pub fn warning(rule_id: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            rule_id: rule_id.map(str::to_string),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BindingDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Error => "error",
        };
        if let Some(rule_id) = &self.rule_id {
            write!(f, "{severity} [{rule_id}]: {}", self.message)
        } else {
            write!(f, "{severity}: {}", self.message)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionTrace {
    pub received: KeyStroke,
    pub scopes: Vec<ScopeKind>,
    pub candidates: Vec<String>,
    pub rejected: Vec<String>,
    pub winner: Option<CommandId>,
}

impl ResolutionTrace {
    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        let scopes = self
            .scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" > ");
        let winner = self
            .winner
            .map_or_else(|| "none".to_string(), |command| command.to_string());
        format!(
            "received: {}\nscopes: {scopes}\ncandidates: {}\nwinner: {winner}",
            self.received,
            if self.candidates.is_empty() {
                "none".to_string()
            } else {
                self.candidates.join(", ")
            }
        )
    }
}

pub(crate) fn conflict(rule_a: &BindingRule, rule_b: &BindingRule) -> BindingDiagnostic {
    BindingDiagnostic::error(
        None,
        format!(
            "conflicting rules '{}' and '{}' for {} in {}",
            rule_a.id, rule_b.id, rule_a.key, rule_a.scope
        ),
    )
}
