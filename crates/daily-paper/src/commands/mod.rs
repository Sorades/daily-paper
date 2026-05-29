mod run;
mod status;

use std::path::PathBuf;

use crate::cli::{Cli, Commands};
use daily_paper_core::config::{default_config_path, default_state_dir, load_config};

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let config_path = cli.config
        .or_else(default_config_path)
        .ok_or_else(|| anyhow::anyhow!(
            "no config file found; specify with --config or create ~/.config/daily-paper/config.toml"
        ))?;

    // Load config to check for [state].dir
    let (raw, _resolved) = load_config(&config_path)
        .map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

    let state_dir = cli.state_dir
        .or_else(|| raw.state.as_ref().and_then(|s| s.dir.as_ref().map(PathBuf::from)))
        .unwrap_or_else(default_state_dir);

    match cli.command {
        Commands::Run(args) => run::execute(&config_path, &state_dir, args).await?,
        Commands::Status(args) => status::execute(&state_dir, args).await?,
    }

    Ok(())
}
