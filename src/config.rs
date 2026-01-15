use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

const CONFIG_FILE: &str = "config.toml";

/// Configuration for docket, stored in `.docket/config.toml`
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Configuration for the `work` command
    #[serde(default)]
    pub work: WorkConfig,

    /// Configuration for bug templates
    #[serde(default)]
    pub templates: TemplatesConfig,
}

/// Configuration for bug templates
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TemplatesConfig {
    /// Default template to use when creating bugs (defaults to "default")
    #[serde(default)]
    pub default: Option<String>,
}

/// Configuration for the `work` command
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorkConfig {
    /// Skip permission prompts when running claude (--dangerously-skip-permissions)
    #[serde(default)]
    pub skip_permissions: bool,

    /// Automatically run the /docket-implement skill on startup
    #[serde(default)]
    pub auto_implement: bool,
}

impl Config {
    /// Load config from a .docket directory, falling back to defaults if not found
    pub fn load(docket_root: &Path) -> Result<Self> {
        let config_path = docket_root.join(CONFIG_FILE);

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
