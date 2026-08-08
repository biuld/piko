use super::merging::*;
use super::*;

#[derive(Debug, Clone)]
pub struct SettingsManager {
    global_path: PathBuf,
    project_path: PathBuf,
    global_settings: HostSettings,
    project_settings: HostSettings,
    overrides: HostSettings,
    merged: HostSettings,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read settings {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse settings {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize settings {path}: {source}")]
    TomlSerialize {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },
}

impl SettingsManager {
    pub fn create(cwd: impl AsRef<Path>) -> Result<Self, SettingsError> {
        Self::create_with_overrides(cwd, HostSettings::default())
    }

    pub fn create_with_overrides(
        cwd: impl AsRef<Path>,
        overrides: HostSettings,
    ) -> Result<Self, SettingsError> {
        let global_path = piko_dir().join("settings.toml");
        let project_path = cwd.as_ref().join(".piko").join("settings.toml");
        Self::from_paths(global_path, project_path, overrides)
    }

    pub fn from_paths(
        global_path: impl Into<PathBuf>,
        project_path: impl Into<PathBuf>,
        overrides: HostSettings,
    ) -> Result<Self, SettingsError> {
        let global_path = global_path.into();
        let project_path = project_path.into();

        if !global_path.exists() && !global_path.as_os_str().is_empty() {
            if let Some(parent) = global_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&global_path, default_settings_template());
        }

        let global_settings = load_from_file(&global_path)?;
        let project_settings = load_from_file(&project_path)?;
        let merged = merge(
            merge(
                merge(default_settings(), global_settings.clone()),
                project_settings.clone(),
            ),
            overrides.clone(),
        );
        Ok(Self {
            global_path,
            project_path,
            global_settings,
            project_settings,
            overrides,
            merged,
        })
    }

    pub fn in_memory(settings: HostSettings) -> Self {
        let merged = merge(default_settings(), settings.clone());
        Self {
            global_path: PathBuf::new(),
            project_path: PathBuf::new(),
            global_settings: HostSettings::default(),
            project_settings: HostSettings::default(),
            overrides: settings,
            merged,
        }
    }

    pub fn reload(&mut self) -> Result<(), SettingsError> {
        self.global_settings = load_from_file(&self.global_path)?;
        self.project_settings = load_from_file(&self.project_path)?;
        self.merged = merge(
            merge(
                merge(default_settings(), self.global_settings.clone()),
                self.project_settings.clone(),
            ),
            self.overrides.clone(),
        );
        Ok(())
    }

    pub fn apply_overrides(&mut self, overrides: HostSettings) {
        self.overrides = merge(self.overrides.clone(), overrides.clone());
        self.merged = merge(self.merged.clone(), overrides);
    }

    pub fn settings(&self) -> HostSettings {
        self.merged.clone()
    }

    pub fn get_default_provider(&self) -> Option<&str> {
        self.merged.default_provider.as_deref()
    }

    pub fn get_default_model(&self) -> Option<&str> {
        self.merged.default_model.as_deref()
    }

    pub fn get_transport(&self) -> &str {
        self.merged.transport.as_deref().unwrap_or("auto")
    }

    pub fn global_path(&self) -> &Path {
        &self.global_path
    }

    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    pub fn get_compaction_settings(&self) -> (bool, u64, u64, u64) {
        let compaction = self.merged.compaction.as_ref();
        (
            compaction
                .and_then(|settings| settings.enabled)
                .unwrap_or(true),
            compaction
                .and_then(|settings| settings.reserve_tokens)
                .unwrap_or(16384),
            compaction
                .and_then(|settings| settings.keep_recent_tokens)
                .unwrap_or(20000),
            compaction
                .and_then(|settings| settings.min_growth_tokens)
                .unwrap_or(DEFAULT_MIN_GROWTH_TOKENS),
        )
    }

    /// Apply a partial update and persist to the project settings file.
    pub fn update_and_persist(&mut self, patch: HostSettings) -> Result<(), SettingsError> {
        self.project_settings = merge(self.project_settings.clone(), patch.clone());
        self.merged = merge(self.merged.clone(), patch);
        if self.project_path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.project_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = toml::to_string_pretty(&self.project_settings).map_err(|source| {
            SettingsError::TomlSerialize {
                path: self.project_path.clone(),
                source,
            }
        })?;
        fs::write(&self.project_path, content).map_err(|source| SettingsError::Io {
            path: self.project_path.clone(),
            source,
        })
    }
}
