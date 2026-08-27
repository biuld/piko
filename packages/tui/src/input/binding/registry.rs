use std::collections::BTreeMap;

use crate::{
    input::{
        binding::{
            BindingDiagnostic, BindingRegistry, BindingRule, BindingRuleSetting, Condition,
            ContextAtom, DiagnosticSeverity, KeyStroke, Propagation, Resolution, RuleSource,
            ScopeKind, ScopeStack,
        },
        command::{CommandId, command_spec},
    },
    terminal::{KeyPhase, TerminalProfile},
};

impl BindingRegistry {
    /// Compile the built-ins and an already host-merged settings object.
    pub fn compile(
        profile: TerminalProfile,
        settings: Option<&super::KeybindingSettings>,
    ) -> Result<Self, Vec<BindingDiagnostic>> {
        let mut builtins = super::defaults::default_rules()
            .into_iter()
            .map(|rule| (rule.id.clone(), rule))
            .collect::<BTreeMap<_, _>>();
        let mut diagnostics = Vec::new();

        if let Some(settings) = settings {
            for (id, setting) in &settings.rules {
                apply_setting(&mut builtins, id, setting, &mut diagnostics);
            }
        }

        let mut rules = builtins.into_values().collect::<Vec<_>>();
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        validate_rules(&rules, &profile, &mut diagnostics);

        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        {
            return Err(diagnostics);
        }

        Ok(Self {
            profile,
            rules,
            diagnostics,
        })
    }

    /// Compile a settings update without making a caller discard its last
    /// valid registry. Invalid user input is retained as diagnostics while
    /// the executable registry falls back to built-ins.
    pub fn compile_with_diagnostics(
        profile: TerminalProfile,
        settings: Option<&super::KeybindingSettings>,
    ) -> Self {
        match Self::compile(profile.clone(), settings) {
            Ok(registry) => registry,
            Err(diagnostics) => {
                let mut fallback = Self::compile(profile, None)
                    .expect("built-in keybinding registry must be valid");
                fallback.diagnostics = diagnostics;
                fallback
            }
        }
    }

    pub fn profile(&self) -> &TerminalProfile {
        &self.profile
    }

    pub fn rules(&self) -> &[BindingRule] {
        &self.rules
    }

    #[allow(dead_code)]
    pub fn diagnostics(&self) -> &[BindingDiagnostic] {
        &self.diagnostics
    }

    #[allow(dead_code)]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    /// Whether this rule's chord can be distinguished by the effective input
    /// profile. The first capability-dependent rule is modified Enter; the
    /// same gate also protects user-defined Shift+Enter rules.
    pub fn reachable(&self, rule: &BindingRule) -> bool {
        if rule.key.key.is_shift_enter(rule.key.modifiers) {
            self.profile.key_reachability.shift_enter()
        } else {
            true
        }
    }

    /// Return a deterministic discoverable key for one command in the current
    /// context. The first active scope wins, matching event resolution.
    /// Multiple aliases for the same command are harmless; a key shared with
    /// a different command is never advertised.
    pub fn binding_for(
        &self,
        command: CommandId,
        context: &super::BindingContext,
        scopes: &ScopeStack,
    ) -> Option<KeyStroke> {
        let spec = command_spec(command)?;
        if !(spec.enablement)(context) {
            return None;
        }
        for scope in scopes.iter() {
            let matches = self
                .rules
                .iter()
                .filter(|rule| {
                    rule_active_for_context(self, rule, context)
                        && rule.command == command
                        && rule.scope == scope.kind
                })
                .collect::<Vec<_>>();
            for candidate in matches {
                let conflict = self.rules.iter().any(|rule| {
                    rule_active_for_context(self, rule, context)
                        && rule.scope == scope.kind
                        && rule.key == candidate.key
                        && rule.command != command
                });
                if !conflict {
                    return Some(candidate.key);
                }
            }
            if self.rules.iter().any(|rule| {
                rule_active_for_context(self, rule, context)
                    && rule.scope == scope.kind
                    && rule.command == command
            }) || matches!(scope.propagation, Propagation::Stop)
            {
                return None;
            }
        }
        None
    }

    pub fn hint_for(
        &self,
        command: CommandId,
        context: &super::BindingContext,
        scopes: &ScopeStack,
    ) -> Option<String> {
        self.binding_for(command, context, scopes)
            .map(KeyStroke::hint)
    }

    #[allow(dead_code)]
    pub fn trace(
        &self,
        key: KeyStroke,
        phase: KeyPhase,
        context: &super::BindingContext,
        scopes: &ScopeStack,
    ) -> super::ResolutionTrace {
        let resolution = self.resolve(key, phase, context, scopes);
        let candidates = self
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.key == key)
            .map(|rule| rule.id.clone())
            .collect::<Vec<_>>();
        let rejected = self
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.key == key)
            .filter(|rule| {
                rule.conditions
                    .iter()
                    .any(|condition| !condition.matches(context))
                    || !self.reachable(rule)
            })
            .map(|rule| rule.id.clone())
            .collect::<Vec<_>>();
        let winner = match resolution {
            Resolution::Command { command, .. } => Some(command),
            _ => None,
        };
        super::ResolutionTrace {
            received: key,
            scopes: scopes.kinds().collect(),
            candidates,
            rejected,
            winner,
        }
    }
}

fn rule_active_for_context(
    registry: &BindingRegistry,
    rule: &BindingRule,
    context: &super::BindingContext,
) -> bool {
    rule.enabled
        && registry.reachable(rule)
        && rule
            .conditions
            .iter()
            .all(|condition| condition.matches(context))
        && command_spec(rule.command).is_some_and(|spec| (spec.enablement)(context))
}

impl Default for BindingRegistry {
    fn default() -> Self {
        Self::compile(TerminalProfile::enhanced_for_test(), None)
            .expect("built-in keybinding registry must be valid")
    }
}

fn apply_setting(
    rules: &mut BTreeMap<String, BindingRule>,
    id: &str,
    setting: &BindingRuleSetting,
    diagnostics: &mut Vec<BindingDiagnostic>,
) {
    if id.trim().is_empty() {
        diagnostics.push(BindingDiagnostic::error(
            None,
            "binding rule ID must not be empty",
        ));
        return;
    }

    if let Some(existing) = rules.get(id).cloned() {
        let mut rule = existing;
        rule.source = RuleSource::HostSettings;
        if let Some(enabled) = setting.enabled {
            rule.enabled = enabled;
        }
        if let Some(raw_key) = &setting.key {
            match KeyStroke::parse(raw_key) {
                Some(key) => rule.key = key,
                None => diagnostics.push(BindingDiagnostic::error(
                    Some(id),
                    format!("invalid key chord '{raw_key}'"),
                )),
            }
        }
        if let Some(raw_command) = &setting.command {
            match CommandId::parse(raw_command) {
                Some(command) => rule.command = command,
                None => diagnostics.push(BindingDiagnostic::error(
                    Some(id),
                    format!("unknown command '{raw_command}'"),
                )),
            }
        }
        if let Some(raw_scope) = &setting.scope {
            match ScopeKind::parse(raw_scope) {
                Some(scope) => rule.scope = scope,
                None => diagnostics.push(BindingDiagnostic::error(
                    Some(id),
                    format!("unknown scope '{raw_scope}'"),
                )),
            }
        }
        if let Some(raw_conditions) = &setting.when
            && let Some(conditions) = parse_conditions(id, raw_conditions, diagnostics)
        {
            rule.conditions = conditions;
        }
        rules.insert(id.to_string(), rule);
        return;
    }

    let Some(raw_key) = setting.key.as_deref() else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            "custom binding requires 'key'",
        ));
        return;
    };
    let Some(raw_command) = setting.command.as_deref() else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            "custom binding requires 'command'",
        ));
        return;
    };
    let Some(raw_scope) = setting.scope.as_deref() else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            "custom binding requires 'scope'",
        ));
        return;
    };
    let Some(key) = KeyStroke::parse(raw_key) else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            format!("invalid key chord '{raw_key}'"),
        ));
        return;
    };
    let Some(command) = CommandId::parse(raw_command) else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            format!("unknown command '{raw_command}'"),
        ));
        return;
    };
    let Some(scope) = ScopeKind::parse(raw_scope) else {
        diagnostics.push(BindingDiagnostic::error(
            Some(id),
            format!("unknown scope '{raw_scope}'"),
        ));
        return;
    };
    let conditions = match setting.when.as_deref() {
        Some(values) => match parse_conditions(id, values, diagnostics) {
            Some(conditions) => conditions,
            None => return,
        },
        None => Vec::new(),
    };
    rules.insert(
        id.to_string(),
        BindingRule {
            id: id.to_string(),
            key,
            command,
            scope,
            conditions,
            source: RuleSource::HostSettings,
            enabled: setting.enabled.unwrap_or(true),
        },
    );
}

fn parse_conditions(
    id: &str,
    values: &[String],
    diagnostics: &mut Vec<BindingDiagnostic>,
) -> Option<Vec<Condition>> {
    let mut conditions = Vec::with_capacity(values.len());
    for value in values {
        let Some((atom, negated)) = ContextAtom::parse(value) else {
            diagnostics.push(BindingDiagnostic::error(
                Some(id),
                format!("unknown context condition '{value}'"),
            ));
            continue;
        };
        if conditions
            .iter()
            .any(|condition: &Condition| condition.atom == atom && condition.negated != negated)
        {
            diagnostics.push(BindingDiagnostic::error(
                Some(id),
                "conditions are contradictory",
            ));
        }
        conditions.push(Condition { atom, negated });
    }
    if diagnostics.iter().any(|diagnostic| {
        diagnostic.rule_id.as_deref() == Some(id)
            && diagnostic.severity == DiagnosticSeverity::Error
    }) {
        None
    } else {
        Some(conditions)
    }
}

fn validate_rules(
    rules: &[BindingRule],
    profile: &TerminalProfile,
    diagnostics: &mut Vec<BindingDiagnostic>,
) {
    for rule in rules {
        if !rule.enabled && rule.source == RuleSource::HostSettings {
            diagnostics.push(BindingDiagnostic::warning(
                Some(&rule.id),
                "binding disabled by host settings",
            ));
        }
        let Some(spec) = command_spec(rule.command) else {
            diagnostics.push(BindingDiagnostic::error(
                Some(&rule.id),
                format!("command '{}' is not in the catalog", rule.command),
            ));
            continue;
        };
        if !spec.scopes.contains(&rule.scope) {
            diagnostics.push(BindingDiagnostic::error(
                Some(&rule.id),
                format!(
                    "command '{}' is not allowed in scope '{}'",
                    rule.command, rule.scope
                ),
            ));
        }
        if rule.enabled
            && rule.key.key.is_shift_enter(rule.key.modifiers)
            && !profile.key_reachability.shift_enter()
        {
            diagnostics.push(BindingDiagnostic::warning(
                Some(&rule.id),
                "binding is unreachable in the effective fallback keyboard profile",
            ));
        }
    }

    for (index, left) in rules.iter().enumerate() {
        if !left.enabled {
            continue;
        }
        for right in rules.iter().skip(index + 1) {
            if !right.enabled
                || left.scope != right.scope
                || !conditions_overlap(&left.conditions, &right.conditions)
            {
                continue;
            }
            if left.key == right.key {
                diagnostics.push(super::diagnostics::conflict(left, right));
            }
        }
    }
}

fn conditions_overlap(left: &[Condition], right: &[Condition]) -> bool {
    !left.iter().any(|left_condition| {
        right.iter().any(|right_condition| {
            left_condition.atom == right_condition.atom
                && left_condition.negated != right_condition.negated
        })
    })
}
