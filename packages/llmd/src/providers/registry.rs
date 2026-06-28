use super::oauth::OAuthFlow;
use super::provider::Provider;

// ============================================================================
// ProviderRegistry — holds Provider and OAuthFlow instances, keyed by id
// ============================================================================

pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, Box<dyn Provider>>,
    oauth_flows: std::collections::HashMap<String, Box<dyn OAuthFlow>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: std::collections::HashMap::new(),
            oauth_flows: std::collections::HashMap::new(),
        };
        // Load built-in providers from TOML catalogs
        for provider in super::toml_provider::loader::load_builtin_providers() {
            registry.register(Box::new(provider));
        }
        // Register built-in OAuth flows
        registry.register_oauth(Box::new(super::oauth::openai::OpenAIProvider::new()));
        registry
    }

    pub fn register(&mut self, provider: Box<dyn Provider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn register_oauth(&mut self, flow: Box<dyn OAuthFlow>) {
        self.oauth_flows.insert(flow.provider_id().to_string(), flow);
    }

    /// Scan `dir` for `*.toml` files and register each as a [`TomlProvider`].
    /// Files that fail to parse are skipped with a warning.
    pub fn load_from_dir(&mut self, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match super::toml_provider::TomlProvider::from_toml(&path) {
                Ok(provider) => {
                    tracing::debug!("Loaded user provider from {}", path.display());
                    self.register(Box::new(provider));
                }
                Err(e) => {
                    tracing::warn!("Skipping {}: {e}", path.display());
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&dyn Provider> {
        self.providers.get(id).map(|p| p.as_ref())
    }

    /// Get the OAuth flow for a provider, if registered.
    pub fn get_oauth(&self, id: &str) -> Option<&dyn OAuthFlow> {
        self.oauth_flows.get(id).map(|f| f.as_ref())
    }

    /// Iterate over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Provider> {
        self.providers.values().map(|p| p.as_ref())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
