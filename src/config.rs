use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub api_token: String,

    #[serde(default)]
    pub autostart: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found at {:?}\n\nPlease create it with your Todoist API token:\n\n  mkdir -p ~/.config/todo-tray\n  echo 'api_token = \"YOUR_TOKEN_HERE\"' > ~/.config/todo-tray/config.toml\n\nGet your API token from: https://app.todoist.com/prefs/integrations",
                config_path
            ));
        }

        let content = fs::read_to_string(&config_path).context("Failed to read config file")?;

        let config: Config = toml::from_str(&content).context(
            "Failed to parse config file. Make sure it contains: api_token = \"your_token\"",
        )?;

        if config.api_token.is_empty() || config.api_token == "YOUR_TOKEN_HERE" {
            return Err(anyhow::anyhow!(
                "Please set your actual Todoist API token in {:?}",
                config_path
            ));
        }

        Ok(config)
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not find config directory")?;
        Ok(config_dir.join("todo-tray").join("config.toml"))
    }

    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not find config directory")?;
        Ok(config_dir.join("todo-tray"))
    }
}
