mod archive;
mod cache;
mod config;
mod run;
mod serve;
mod status;

use crate::cli::{Cli, Commands};
use daily_paper_core::config::default_data_dir;

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let data_dir = cli.directory.unwrap_or_else(default_data_dir);

    match cli.command {
        Commands::Run(args) => run::execute(&data_dir, args).await?,
        Commands::Status(args) => status::execute(&data_dir, args).await?,
        Commands::Serve(args) => serve::execute(&data_dir, args).await?,
        Commands::Config(cmd) => match cmd {
            crate::cli::ConfigCommands::Show => config::show(&data_dir)?,
            crate::cli::ConfigCommands::Path => config::path(&data_dir),
            crate::cli::ConfigCommands::Init => config::init(&data_dir)?,
        },
        Commands::Cache(cmd) => match cmd {
            crate::cli::CacheCommands::List => cache::list(&data_dir)?,
            crate::cli::CacheCommands::Clean(args) => cache::clean(&data_dir, &args.kind)?,
            crate::cli::CacheCommands::Size => cache::size(&data_dir)?,
        },
        Commands::Archive(cmd) => match cmd {
            crate::cli::ArchiveCommands::List => archive::list(&data_dir)?,
            crate::cli::ArchiveCommands::Show(args) => archive::show(&data_dir, &args.date)?,
        },
    }

    Ok(())
}
