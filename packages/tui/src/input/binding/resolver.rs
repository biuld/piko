use super::{BindingContext, BindingRegistry, BindingRule, ScopeStack};
use crate::{
    input::command::{CommandId, RepeatPolicy, TerminalRequirement},
    terminal::KeyPhase,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    Command {
        command: CommandId,
        rule_id: String,
    },
    Conflict {
        key: String,
        scope: String,
        rule_ids: Vec<String>,
    },
    Consumed,
    Unhandled,
}

impl BindingRegistry {
    pub fn resolve(
        &self,
        key: crate::input::binding::KeyStroke,
        phase: KeyPhase,
        context: &BindingContext,
        scopes: &ScopeStack,
    ) -> Resolution {
        for scope in scopes.iter() {
            let candidates: Vec<&BindingRule> = self
                .rules()
                .iter()
                .filter(|rule| {
                    rule.enabled
                        && rule.scope == scope.kind
                        && rule.key == key
                        && rule
                            .conditions
                            .iter()
                            .all(|condition| condition.matches(context))
                        && self.reachable(rule)
                        && command_enabled(rule.command, context, phase)
                })
                .collect();

            if candidates.is_empty() {
                if matches!(scope.propagation, super::Propagation::Stop) {
                    return Resolution::Consumed;
                }
                continue;
            }
            if candidates.len() > 1 {
                return Resolution::Conflict {
                    key: key.to_string().to_ascii_lowercase(),
                    scope: scope.kind.to_string(),
                    rule_ids: candidates.iter().map(|rule| rule.id.clone()).collect(),
                };
            }
            let rule = candidates[0];
            return Resolution::Command {
                command: rule.command,
                rule_id: rule.id.clone(),
            };
        }
        Resolution::Unhandled
    }
}

fn command_enabled(command: CommandId, context: &BindingContext, phase: KeyPhase) -> bool {
    let Some(spec) = crate::input::command::command_spec(command) else {
        return false;
    };
    if phase == KeyPhase::Repeat && spec.repeat == RepeatPolicy::PressOnly {
        return false;
    }
    if spec.terminal_requirement == Some(TerminalRequirement::EnhancedKeyboard)
        && !context.terminal_enhanced
    {
        return false;
    }
    (spec.enablement)(context)
}
