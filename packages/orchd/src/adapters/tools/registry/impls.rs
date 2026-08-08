use super::*;

impl ToolRegistryImpl {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            tool_sets: RwLock::new(HashMap::new()),
            approval_gateway: RwLock::new(None),
            features: RwLock::new(None),
        }
    }

    // ---- Singleton registration ----

    /// Register a tool provider.
    pub async fn register_provider(&self, provider: Box<dyn ToolProvider>) {
        let id = provider.id().to_string();
        self.providers.write().await.insert(id, provider);
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
        let features = self.features.read().await;
        disabled_feature_for_tool_name(features.as_ref(), tool_name).map(str::to_string)
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
