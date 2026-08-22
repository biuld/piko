use super::*;

impl ToolRegistryImpl {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            tool_sets: RwLock::new(HashMap::new()),
            approval_gateway: RwLock::new(None),
            features: RwLock::new(None),
            upstream_tools: RwLock::new(HashMap::new()),
        }
    }

    // ---- Singleton registration ----

    /// Register a tool provider.
    pub async fn register_provider(&self, provider: Box<dyn ToolProvider>) {
        let id = provider.id().to_string();
        self.providers.write().await.insert(id, provider);
    }

    /// Validate and publish a provider and its tool sets as one contribution.
    /// Existing identifiers are never replaced through this path.
    pub async fn install_contribution(&self, contribution: ToolContribution) -> Result<(), String> {
        let provider_id = contribution.provider.id().to_string();
        if provider_id.trim().is_empty() {
            return Err("tool provider id cannot be empty".into());
        }
        let mut set_ids = HashSet::new();
        for tool_set in &contribution.tool_sets {
            if tool_set.id.trim().is_empty() {
                return Err("tool set id cannot be empty".into());
            }
            if !set_ids.insert(tool_set.id.clone()) {
                return Err(format!(
                    "duplicate tool set id in contribution: {}",
                    tool_set.id
                ));
            }
            for tool_ref in &tool_set.tools {
                let referenced_provider = match tool_ref {
                    ToolSetToolRef::ProviderTool { provider_id, .. }
                    | ToolSetToolRef::ProviderNamespace { provider_id, .. } => provider_id,
                    ToolSetToolRef::OrchestratorControl { .. } => "orch",
                };
                if referenced_provider != provider_id && referenced_provider != "orch" {
                    return Err(format!(
                        "tool set {} references provider {}, expected {}",
                        tool_set.id, referenced_provider, provider_id
                    ));
                }
            }
        }

        // Lock in catalog read order so discovery cannot observe a partial
        // contribution between provider and set publication.
        let mut providers = self.providers.write().await;
        let mut tool_sets = self.tool_sets.write().await;
        if providers.contains_key(&provider_id) {
            return Err(format!("tool provider already registered: {provider_id}"));
        }
        if let Some(existing) = contribution
            .tool_sets
            .iter()
            .find(|tool_set| tool_sets.contains_key(&tool_set.id))
        {
            return Err(format!("tool set already registered: {}", existing.id));
        }
        providers.insert(provider_id, contribution.provider);
        for tool_set in contribution.tool_sets {
            tool_sets.insert(tool_set.id.clone(), tool_set);
        }
        Ok(())
    }

    /// Unregister a tool provider by ID.
    pub async fn unregister_provider(&self, provider_id: &str) {
        self.providers.write().await.remove(provider_id);
    }

    /// Register a tool set.
    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.tool_sets
            .write()
            .await
            .insert(tool_set.id.clone(), tool_set);
    }

    /// Unregister a tool set by ID.
    pub async fn unregister_tool_set(&self, tool_set_id: &str) {
        self.tool_sets.write().await.remove(tool_set_id);
    }

    /// Set (or clear) the approval gateway.
    pub async fn set_approval_gateway(&self, gateway: Option<Box<dyn ApprovalGateway>>) {
        *self.approval_gateway.write().await = gateway;
    }

    /// Install the resolved managed-feature set (F-18). `None` keeps every
    /// feature enabled (today's behavior).
    pub async fn set_features(&self, features: Option<HashMap<String, bool>>) {
        *self.features.write().await = features;
    }

    /// Return the feature key that disables `tool_name`, if any. Used by the
    /// direct-call error path when a tool has no route.
    pub async fn feature_gate(&self, tool_name: &str) -> Option<String> {
        let tool_set_ids = self.tool_sets.read().await.keys().cloned().collect();
        let context = ToolDiscoveryContext {
            agent_id: String::new(),
            agent_instance_id: None,
            tool_set_ids,
            active_tool_names: None,
        };
        let catalog = self.build_catalog(&context).await.ok()?;
        let feature = catalog
            .iter()
            .find(|entry| entry.public_name == tool_name)?
            .feature
            .as_ref()?;
        let features = self.features.read().await;
        (!features
            .as_ref()
            .and_then(|values| values.get(feature))
            .copied()
            .unwrap_or(true))
        .then(|| feature.clone())
    }

    /// List all registered tool sets.
    pub async fn list_tool_sets(&self) -> std::collections::HashMap<String, ToolSet> {
        self.tool_sets.read().await.clone()
    }

    // ---- Catalog building ----

    /// Build the full tool catalog from registered providers and tool sets.
    pub(super) async fn build_catalog(
        &self,
        context: &ToolDiscoveryContext,
    ) -> Result<Vec<CatalogEntry>, String> {
        let providers = self.providers.read().await;
        let tool_sets = self.tool_sets.read().await;

        let mut entries: Vec<CatalogEntry> = vec![];
        let mut seen: HashSet<String> = HashSet::new();
        let mut duplicates: HashSet<String> = HashSet::new();
        let mut provider_cache: HashMap<String, Vec<ToolDef>> = HashMap::new();

        // Helper: discover tools from a provider (with caching).
        async fn discover_from<'a>(
            provider_id: &str,
            cache: &mut HashMap<String, Vec<ToolDef>>,
            providers: &tokio::sync::RwLockReadGuard<'a, HashMap<String, Box<dyn ToolProvider>>>,
            ctx: &ToolDiscoveryContext,
        ) -> Vec<ToolDef> {
            if let Some(cached) = cache.get(provider_id) {
                return cached.clone();
            }
            if let Some(p) = providers.get(provider_id) {
                let tools = p
                    .discover(ToolDiscoveryContext {
                        agent_id: ctx.agent_id.clone(),
                        agent_instance_id: ctx.agent_instance_id.clone(),
                        tool_set_ids: vec![],
                        active_tool_names: None,
                    })
                    .await;
                cache.insert(provider_id.to_string(), tools.clone());
                return tools;
            }
            vec![]
        }

        // Process each tool set reference
        for tool_set in tool_sets.values() {
            if !context.tool_set_ids.contains(&tool_set.id) {
                continue;
            }

            for tool_ref in &tool_set.tools {
                let policy = merge_policy(tool_set.policy.as_ref(), tool_ref_policy(tool_ref));

                match tool_ref {
                    ToolSetToolRef::ProviderTool {
                        provider_id,
                        tool_name,
                        alias,
                        ..
                    } => {
                        let tools =
                            discover_from(provider_id, &mut provider_cache, &providers, context)
                                .await;
                        if let Some(td) = tools.iter().find(|t| t.name == *tool_name) {
                            let public_name = alias.as_ref().unwrap_or(tool_name);
                            add_entry(
                                &mut entries,
                                &mut seen,
                                &mut duplicates,
                                public_name,
                                provider_id,
                                tool_name,
                                tool_set
                                    .feature
                                    .as_ref()
                                    .and_then(|feature| feature.for_tool(tool_name)),
                                td,
                                policy.as_ref(),
                                tool_set.policy.as_ref(),
                            );
                        }
                    }
                    ToolSetToolRef::OrchestratorControl { action, alias, .. } => {
                        let tools =
                            discover_from("orch", &mut provider_cache, &providers, context).await;
                        if let Some(td) = tools.iter().find(|t| t.name == *action) {
                            let public_name = alias.as_ref().unwrap_or(action);
                            add_entry(
                                &mut entries,
                                &mut seen,
                                &mut duplicates,
                                public_name,
                                "orch",
                                action,
                                tool_set
                                    .feature
                                    .as_ref()
                                    .and_then(|feature| feature.for_tool(action)),
                                td,
                                policy.as_ref(),
                                tool_set.policy.as_ref(),
                            );
                        }
                    }
                    ToolSetToolRef::ProviderNamespace {
                        provider_id,
                        namespace,
                        alias,
                        ..
                    } => {
                        let tools =
                            discover_from(provider_id, &mut provider_cache, &providers, context)
                                .await;
                        for td in &tools {
                            if td.name.starts_with(namespace.as_str()) {
                                let base_name = &td.name[namespace.len()..];
                                let public_name = if let Some(a) = alias {
                                    format!("{a}{base_name}")
                                } else {
                                    td.name.clone()
                                };
                                add_entry(
                                    &mut entries,
                                    &mut seen,
                                    &mut duplicates,
                                    &public_name,
                                    provider_id,
                                    &td.name,
                                    tool_set
                                        .feature
                                        .as_ref()
                                        .and_then(|feature| feature.for_tool(&td.name)),
                                    td,
                                    policy.as_ref(),
                                    tool_set.policy.as_ref(),
                                );
                            }
                        }
                    }
                }
            }
        }

        if !duplicates.is_empty() {
            let mut dup_list: Vec<_> = duplicates.iter().cloned().collect();
            dup_list.sort();
            return Err(format!(
                "Duplicate tool names in catalog: {}",
                dup_list.join(", ")
            ));
        }

        Ok(entries)
    }
}
