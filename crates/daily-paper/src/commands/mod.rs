mod archive;
mod cache;
mod config;
mod run;
pub(crate) mod serve;
mod status;
mod systemd;

use crate::cli::{Cli, Commands};
use daily_paper_core::config::default_data_dir;

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let data_dir = cli.directory.unwrap_or_else(default_data_dir);

    match cli.command {
        // No subcommand → start web server + scheduler
        None => serve::execute(&data_dir, cli.port, cli.no_schedule).await?,
        Some(Commands::Run(args)) => {
            init_tracing();
            run::execute(&data_dir, args).await?
        }
        Some(Commands::Status(args)) => {
            init_tracing();
            status::execute(&data_dir, args).await?
        }
        Some(Commands::Config(cmd)) => {
            init_tracing();
            match cmd {
                crate::cli::ConfigCommands::Show => config::show(&data_dir)?,
                crate::cli::ConfigCommands::Path => config::path(&data_dir),
                crate::cli::ConfigCommands::Init => config::init(&data_dir)?,
            }
        }
        Some(Commands::Cache(cmd)) => {
            init_tracing();
            match cmd {
                crate::cli::CacheCommands::List => cache::list(&data_dir)?,
                crate::cli::CacheCommands::Clean(args) => cache::clean(&data_dir, &args.kind)?,
                crate::cli::CacheCommands::Size => cache::size(&data_dir)?,
            }
        }
        Some(Commands::Archive(cmd)) => {
            init_tracing();
            match cmd {
                crate::cli::ArchiveCommands::List => archive::list(&data_dir)?,
                crate::cli::ArchiveCommands::Show(args) => archive::show(&data_dir, &args.date)?,
            }
        }
        Some(Commands::Systemd(cmd)) => {
            init_tracing();
            match cmd {
                crate::cli::SystemdCommands::Install => systemd::install(&data_dir)?,
                crate::cli::SystemdCommands::Uninstall => systemd::uninstall(&data_dir)?,
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
