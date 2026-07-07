//! Configuration and API key management.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_MAX_TOKENS: u32 = 8192;
const DEFAULT_THINKING_BUDGET: u32 = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub anthropic_api_key: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub thinking_enabled: bool,
    pub thinking_budget_tokens: u32,
    pub workspace: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            anthropic_api_key: None,
            model: DEFAULT_MODEL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
            thinking_enabled: false,
            thinking_budget_tokens: DEFAULT_THINKING_BUDGET,
            workspace: None,
        }
    }
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("could not resolve home config directory")?
            .join("landingpig");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut config = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse {}", path.display()))?
        } else {
            Config::default()
        };

        if config.anthropic_api_key.is_none() {
            if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                if !key.is_empty() {
                    config.anthropic_api_key = Some(key);
                }
            }
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn api_key(&self) -> Option<&str> {
        self.anthropic_api_key.as_deref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.api_key().is_some()
    }
}
