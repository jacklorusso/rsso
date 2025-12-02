use anyhow::Result;
use dirs::{config_dir, data_dir};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Shape of config.toml on disk
#[derive(Debug, Deserialize)]
pub struct GeneralConfig {
    pub default_limit: Option<usize>,
    pub refresh_age_mins: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PathsConfig {
    pub state_file: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub general: Option<GeneralConfig>,
    pub paths: Option<PathsConfig>,
}

/// Resolved config used by the app
#[derive(Debug, Clone)]
pub struct Config {
    pub default_limit: usize,
    pub refresh_age_mins: u64,
    pub state_path: PathBuf,
}

/// Load config from ~/.config/rsso/config.toml if it exists,
/// otherwise use sensible defaults.
pub fn load_config() -> Result<Config> {
    let config_path = config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rsso")
        .join("config.toml");

    let mut cfg_file: Option<ConfigFile> = None;

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        cfg_file = Some(toml::from_str(&contents)?);
    }

    let default_limit = cfg_file
        .as_ref()
        .and_then(|c| c.general.as_ref()?.default_limit)
        .unwrap_or(20);

    let refresh_age_mins = cfg_file
        .as_ref()
        .and_then(|c| c.general.as_ref()?.refresh_age_mins)
        .unwrap_or(60);

    let state_path = cfg_file
        .as_ref()
        .and_then(|c| c.paths.as_ref()?.state_file.clone())
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
        state_path,
    })
}
