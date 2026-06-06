use super::raw::RawConfig;
use super::resolved::ResolvedConfig;
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Load and resolve configuration from a TOML file.
pub fn load_config(path: &Path) -> Result<(RawConfig, ResolvedConfig)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "failed to read config file '{}': {}",
            path.display(),
            e
        ))
    })?;

    let raw: RawConfig = toml::from_str(&content)?;
    let resolved = ResolvedConfig::from_raw(&raw)?;
    Ok((raw, resolved))
}

/// Find the default data directory.
///
/// Automatically selects based on build profile:
/// - Debug build (`cargo run`): `~/.config/daily-paper-dev`
/// - Release build (`cargo build --release`): `~/.config/daily-paper`
///
/// Platform-specific paths:
/// - Linux: `~/.config/<name>`
/// - macOS: `~/Library/Application Support/<name>`
/// - Windows: `%APPDATA%/<name>`
pub fn default_data_dir() -> PathBuf {
    let name = if cfg!(debug_assertions) {
        "daily-paper-dev"
    } else {
        "daily-paper"
    };

    directories::ProjectDirs::from("", "", name)
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(format!(".config/{}", name)))
}
