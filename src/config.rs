//! Configuration management

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(alias = "api_token")]
    pub todoist_api_token: String,

    #[serde(default)]
    pub linear_api_token: Option<String>,

    #[serde(default)]
    pub github_accounts: Vec<GithubAccountConfig>,

    #[serde(default)]
    pub calendar_feeds: Vec<CalendarFeedConfig>,

    #[serde(default = "default_snooze_durations")]
    pub snooze_durations: Vec<String>,

    #[serde(default)]
    pub autostart: bool,
}

/// GitHub account configuration
#[derive(Debug, Deserialize, Clone)]
pub struct GithubAccountConfig {
    pub name: String,
    pub token: String,
}

/// iCal feed configuration
#[derive(Debug, Deserialize, Clone)]
pub struct CalendarFeedConfig {
    pub name: String,
    #[serde(alias = "url")]
    pub ical_url: String,
}

pub fn default_snooze_durations() -> Vec<String> {
    vec!["30m".to_string(), "1d".to_string()]
}

impl Config {
    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found at {:?}\n\n\
                Please create it with your Todoist API token:\n\n\
                mkdir -p ~/Library/Application\\ Support/todo-tray\n\
                echo 'todoist_api_token = \"YOUR_TOKEN_HERE\"' > ~/Library/Application\\ Support/todo-tray/config.toml\n\
                # Optional: linear_api_token = \"YOUR_LINEAR_API_KEY\"\n\n\
                # Optional: multiple GitHub accounts\n\
                [[github_accounts]]\n\
                name = \"work\"\n\
                token = \"ghp_...\"\n\n\
                # Optional: iCal feeds (supports Google Calendar private ICS URLs)\n\
                [[calendar_feeds]]\n\
                name = \"Work Calendar\"\n\
                ical_url = \"https://calendar.google.com/calendar/ical/.../basic.ics\"\n\n\
                # Optional: todoist snooze durations (default: 30m, 1d)\n\
                snooze_durations = [\"30m\", \"1d\"]\n\n\
                Get your API token from: https://app.todoist.com/prefs/integrations",
                config_path
            ));
        }

        let content = fs::read_to_string(&config_path).context("Failed to read config file")?;

        let config: Config = toml::from_str(&content).map_err(|err| {
            anyhow::anyhow!(
                "Failed to parse config file at {:?}: {}",
                config_path,
                err
            )
        })?;

        if config.todoist_api_token.is_empty() || config.todoist_api_token == "YOUR_TOKEN_HERE" {
            return Err(anyhow::anyhow!(
                "Please set your actual Todoist API token in {:?}",
                config_path
            ));
        }

        let mut seen_names = HashSet::new();
        for account in &config.github_accounts {
            let name = account.name.trim();
            let token = account.token.trim();

            if name.is_empty() {
                return Err(anyhow::anyhow!(
                    "GitHub account name cannot be empty in {:?}",
                    config_path
                ));
            }

            if token.is_empty() {
                return Err(anyhow::anyhow!(
                    "GitHub token for account '{}' cannot be empty in {:?}",
                    name,
                    config_path
                ));
            }

            let key = name.to_lowercase();
            if !seen_names.insert(key) {
                return Err(anyhow::anyhow!(
                    "Duplicate GitHub account name '{}' in {:?}",
                    name,
                    config_path
                ));
            }
        }

        let mut seen_calendar_names = HashSet::new();
        for feed in &config.calendar_feeds {
            let name = feed.name.trim();
            let ical_url = feed.ical_url.trim();

            if name.is_empty() {
                return Err(anyhow::anyhow!(
                    "Calendar feed name cannot be empty in {:?}",
                    config_path
                ));
            }

            if ical_url.is_empty() {
                return Err(anyhow::anyhow!(
                    "Calendar iCal URL for feed '{}' cannot be empty in {:?}",
                    name,
                    config_path
                ));
            }

            let key = name.to_lowercase();
            if !seen_calendar_names.insert(key) {
                return Err(anyhow::anyhow!(
                    "Duplicate calendar feed name '{}' in {:?}",
                    name,
                    config_path
                ));
            }
        }

        Ok(config)
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not find config directory")?;
        Ok(config_dir.join("todo-tray").join("config.toml"))
    }
}
