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

/// Find the default config file path.
///
/// Priority:
/// 1. `.daily-paper/config/config.toml` in current directory (project mode)
/// 2. Platform config directory (system mode)
pub fn default_config_path() -> Option<PathBuf> {
    // Check for project-local config first
    let project_config = Path::new(".daily-paper/config/config.toml");
    if project_config.exists() {
        return Some(project_config.to_path_buf());
    }

    // Fall back to platform config directory
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

/// Find the default state directory.
///
/// Priority:
/// 1. `.daily-paper/state` in current directory (project mode)
/// 2. Platform data directory (system mode)
pub fn default_state_dir() -> PathBuf {
    // Check for project-local directory
    let project_dir = Path::new(".daily-paper");
    if project_dir.exists() {
        return project_dir.join("state");
    }

    // Fall back to platform data directory
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".daily-paper"))
}

/// Check if we're in project mode (`.daily-paper/` exists in current directory).
pub fn is_project_mode() -> bool {
    Path::new(".daily-paper").exists()
}
