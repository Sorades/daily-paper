mod archive;
mod cache;
mod config;
mod run;
pub(crate) mod serve;
mod status;

use crate::cli::{Cli, Commands};
use daily_paper_core::config::default_data_dir;

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let data_dir = cli.directory.unwrap_or_else(default_data_dir);

    match cli.command {
        Commands::Run(args) => {
            init_tracing();
            run::execute(&data_dir, args).await?
        }
        Commands::Status(args) => {
            init_tracing();
            status::execute(&data_dir, args).await?
        }
        Commands::Serve(args) => {
            // serve::execute installs its own subscriber with WebLogLayer
            serve::execute(&data_dir, args).await?
        }
        Commands::Config(cmd) => {
            init_tracing();
            match cmd {
                crate::cli::ConfigCommands::Show => config::show(&data_dir)?,
                crate::cli::ConfigCommands::Path => config::path(&data_dir),
                crate::cli::ConfigCommands::Init => config::init(&data_dir)?,
            }
        }
        Commands::Cache(cmd) => {
            init_tracing();
            match cmd {
                crate::cli::CacheCommands::List => cache::list(&data_dir)?,
                crate::cli::CacheCommands::Clean(args) => cache::clean(&data_dir, &args.kind)?,
                crate::cli::CacheCommands::Size => cache::size(&data_dir)?,
            }
        }
        Commands::Archive(cmd) => {
            init_tracing();
            match cmd {
                crate::cli::ArchiveCommands::List => archive::list(&data_dir)?,
                crate::cli::ArchiveCommands::Show(args) => archive::show(&data_dir, &args.date)?,
            }
        }
    }

    Ok(())
}

/// Simple tracing subscriber for non-serve commands.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
