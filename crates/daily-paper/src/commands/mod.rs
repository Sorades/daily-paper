mod run;
mod status;

use crate::cli::{Cli, Commands};
use daily_paper_core::config::{default_config_path, default_state_dir};

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let config_path = cli.config
        .or_else(default_config_path)
        .ok_or_else(|| anyhow::anyhow!(
            "no config file found; specify with --config or create ~/.config/daily-paper/config.toml"
        ))?;

    let state_dir = cli.state_dir.unwrap_or_else(default_state_dir);

    match cli.command {
        Commands::Run(args) => run::execute(&config_path, &state_dir, args).await?,
        Commands::Status(args) => status::execute(&state_dir, args).await?,
    }

    Ok(())
}
