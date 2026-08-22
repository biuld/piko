use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ModelToolSurface {
    pub tools: Vec<piko_llmd::tools::InferenceTool>,
    pub digest: String,
}

impl ToolRegistryImpl {
    pub(crate) async fn register_upstream_catalog(
        &self,
        descriptor: &piko_llmd::gateway::ModelDescriptor,
    ) {
        let key = model_key(&descriptor.model.provider, &descriptor.model.model);
        self.upstream_tools
            .write()
            .await
            .insert(key, descriptor.upstream_tools.clone());
    }

    /// Resolve the complete model-facing surface in one place. Caller tools
    /// retain local routes; upstream entries are definitions only.
    pub(crate) async fn resolve_model_surface(
        &self,
        provider: &str,
        model: &str,
        caller_tools: Vec<ToolDef>,
        allow_upstream: bool,
    ) -> Result<ModelToolSurface, String> {
        let mut tools = caller_tools
            .into_iter()
            .map(piko_llmd::tools::InferenceTool::Caller)
            .collect::<Vec<_>>();
        let mut upstream_descriptors = Vec::new();
        if allow_upstream {
            let key = model_key(provider, model);
            if let Some(upstream) = self.upstream_tools.read().await.get(&key) {
                upstream_descriptors = upstream.clone();
                tools.extend(upstream.iter().map(|definition| {
                    piko_llmd::tools::InferenceTool::Upstream(
                        piko_llmd::tools::UpstreamToolDefinition {
                            name: definition.name.clone(),
                            kind: definition.kind.clone(),
                            resources: Vec::new(),
                            approval: definition.approval,
                        },
                    )
                }));
            }
        }
        upstream_descriptors.sort_by(|left, right| left.name.cmp(&right.name));
        tools.sort_by(|left, right| left.name().cmp(right.name()));
        if let Some(pair) = tools
            .windows(2)
            .find(|pair| pair[0].name() == pair[1].name())
        {
            return Err(format!(
                "duplicate model-visible tool name: {}",
                pair[0].name()
            ));
        }
        let serialized = serde_json::to_string(&(&tools, &upstream_descriptors))
            .map_err(|error| format!("failed to serialize model tool surface: {error}"))?;
        Ok(ModelToolSurface {
            digest: piko_orchd_api::stable_internal_id("model-tool-surface", &[&serialized]),
            tools,
        })
    }
}

fn model_key(provider: &str, model: &str) -> String {
    format!("{provider}/{model}")
}
