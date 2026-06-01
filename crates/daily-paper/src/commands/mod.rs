mod run;
mod serve;
mod status;

use std::path::{Path, PathBuf};

use crate::cli::{Cli, Commands};
use daily_paper_core::config::{
    default_config_path, default_state_dir, init_project_dir, is_project_mode, load_config,
};

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    // Handle init command first (doesn't need config)
    if let Commands::Init = &cli.command {
        let config_dir = Path::new(".daily-paper/config");
        if config_dir.exists() {
            print!(".daily-paper/config/ already exists. Overwrite? [y/N] ");
            use std::io::{self, Write};
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
            std::fs::remove_dir_all(config_dir)?;
        }
        init_project_dir()?;
        println!("Initialized config at .daily-paper/config/");
        println!();
        println!("Directory structure:");
        println!("  .daily-paper/");
        println!("  ├── config/");
        println!("  │   ├── config.toml        # Edit this with your settings");
        println!("  │   └── templates/");
        println!("  │       ├── system_prompt.txt");
        println!("  │       └── report.html");
        println!("  └── state/                 # Auto-managed (cache, runs, reports)");
        println!();
        println!("Next steps:");
        println!("  1. Edit .daily-paper/config/config.toml");
        println!("  2. Run: cargo run -- run --dry-run");
        return Ok(());
    }

    // Serve command loads config for web settings
    if let Commands::Serve(args) = &cli.command {
        let config_path = cli
            .config
            .or_else(default_config_path)
            .ok_or_else(|| anyhow::anyhow!("no config file found; specify with --config"))?;
        let (_raw, resolved) = load_config(&config_path)
            .map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;
        let state_dir = cli
            .state_dir
            .or_else(|| {
                _raw.state
                    .as_ref()
                    .and_then(|s| s.dir.as_ref().map(PathBuf::from))
            })
            .unwrap_or_else(default_state_dir);
        return serve::execute(&state_dir, &config_path, resolved, args.clone()).await;
    }

    let config_path = cli.config
        .or_else(default_config_path)
        .ok_or_else(|| {
            if is_project_mode() {
                anyhow::anyhow!(
                    "config file not found in .daily-paper/; run 'cargo run -- init' to create default config"
                )
            } else {
                anyhow::anyhow!(
                    "no config file found; specify with --config or run 'cargo run -- init' to create project directory"
                )
            }
        })?;

    // Load config to check for [state].dir
    let (raw, _resolved) =
        load_config(&config_path).map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

    let state_dir = cli
        .state_dir
        .or_else(|| {
            raw.state
                .as_ref()
                .and_then(|s| s.dir.as_ref().map(PathBuf::from))
        })
        .unwrap_or_else(default_state_dir);

    match cli.command {
        Commands::Run(args) => run::execute(&config_path, &state_dir, args).await?,
        Commands::Status(args) => status::execute(&state_dir, args).await?,
        Commands::Init => unreachable!(),
        Commands::Serve(_) => unreachable!(),
    }

    Ok(())
}
