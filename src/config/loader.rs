use super::model::Config;
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Load and validate configuration directly from a TOML file.
pub fn load_config(path: &Path) -> Result<(Config, Config)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "failed to read config file '{}': {}",
            path.display(),
            e
        ))
    })?;

    parse_config_str(&content)
}

/// Parse and validate configuration from a TOML string.
pub fn parse_config_str(content: &str) -> Result<(Config, Config)> {
    let cfg: Config = toml::from_str(content)?;
    cfg.validate()?;
    Ok((cfg.clone(), cfg))
}

/// Validate configuration from a TOML string.
pub fn validate_config_str(content: &str) -> Result<()> {
    let cfg: Config = toml::from_str(content)?;
    cfg.validate()?;
    Ok(())
}

/// Find the default data directory.
///
/// Automatically selects based on build profile:
/// - Debug build (`cargo run`): `~/.config/daily-paper-dev`
/// - Release build (`cargo build --release`): `~/.config/daily-paper`
///
/// Can be overridden via `--directory` CLI argument.
pub fn default_data_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        directories::ProjectDirs::from("com", "daily-paper", "daily-paper-dev")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".daily-paper-dev"))
    }

    #[cfg(not(debug_assertions))]
    {
        directories::ProjectDirs::from("com", "daily-paper", "daily-paper")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".daily-paper"))
    }
}
