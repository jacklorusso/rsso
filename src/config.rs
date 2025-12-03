use anyhow::Result;
use dirs::{config_dir, data_dir};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Shape of config.toml on disk
///
/// Example:
/// default_limit = 5
/// refresh_age_mins = 60
/// new_line_between_items = false
/// state_file = "/some/custom/path.json"
#[derive(Debug, Deserialize)]
pub struct RawConfig {
    pub default_limit: Option<usize>,
    pub refresh_age_mins: Option<u64>,
    pub new_line_between_items: Option<bool>,
    pub state_file: Option<String>,
}

/// Resolved config used by the app
#[derive(Debug, Clone)]
pub struct Config {
    pub default_limit: usize,
    pub refresh_age_mins: u64,
    pub new_line_between_items: bool,
    pub state_path: PathBuf,
}

/// Load config from ~/.config/rsso/config.toml if it exists,
/// otherwise use sensible defaults:
///
/// default_limit = 5
/// refresh_age_mins = 60
/// new_line_between_items = false
/// state_file = "/path/to/state.json"
pub fn load_config() -> Result<Config> {
    let config_path = config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rsso")
        .join("config.toml");

    let mut raw: Option<RawConfig> = None;

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        raw = Some(toml::from_str(&contents)?);
    }

    let default_limit = raw.as_ref().and_then(|c| c.default_limit).unwrap_or(20);

    let refresh_age_mins = raw.as_ref().and_then(|c| c.refresh_age_mins).unwrap_or(60);

    let new_line_between_items = raw
        .as_ref()
        .and_then(|c| c.new_line_between_items)
        .unwrap_or(false);

    let state_path = raw
        .as_ref()
        .and_then(|c| c.state_file.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("rsso")
                .join("state.json")
        });

    Ok(Config {
        default_limit,
        refresh_age_mins,
        new_line_between_items,
        state_path,
    })
}
