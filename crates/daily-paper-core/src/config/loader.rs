use crate::error::{Error, Result};
use super::raw::RawConfig;
use super::resolved::ResolvedConfig;
use std::path::Path;

/// Load and resolve configuration from a TOML file.
pub fn load_config(path: &Path) -> Result<(RawConfig, ResolvedConfig)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!("failed to read config file '{}': {}", path.display(), e))
    })?;

    let raw: RawConfig = toml::from_str(&content)?;
    let resolved = ResolvedConfig::from_raw(&raw)?;
    Ok((raw, resolved))
}

/// Find the default config file path for the current platform.
pub fn default_config_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

/// Find the default state directory for the current platform.
pub fn default_state_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".daily-paper"))
}
