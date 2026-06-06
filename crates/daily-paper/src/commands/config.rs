use std::path::Path;

use anyhow::Result;
use daily_paper_core::config::load_config;

/// Show current configuration (with secrets masked).
pub fn show(data_dir: &Path) -> Result<()> {
    let config_path = data_dir.join("config.toml");
    if !config_path.exists() {
        anyhow::bail!(
            "config file not found at '{}'; run `daily-paper config init` to create one",
            config_path.display()
        );
    }

    let (raw, _resolved) = load_config(&config_path)?;
    let content = toml::to_string_pretty(&raw)?;
    println!("{}", content);
    Ok(())
}

/// Show config file path.
pub fn path(data_dir: &Path) {
    let config_path = data_dir.join("config.toml");
    println!("{}", config_path.display());
}

/// Initialize configuration file from example.
pub fn init(data_dir: &Path) -> Result<()> {
    let config_path = data_dir.join("config.toml");
    if config_path.exists() {
        anyhow::bail!(
            "config file already exists at '{}'; remove it first or edit manually",
            config_path.display()
        );
    }

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(data_dir)?;

    // Write example config
    std::fs::write(
        &config_path,
        include_str!("../../../../config.example.toml"),
    )?;
    println!("Created config file at '{}'", config_path.display());
    Ok(())
}
