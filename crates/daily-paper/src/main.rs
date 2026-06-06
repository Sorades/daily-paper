mod cli;
mod commands;
mod log_layer;
mod progress;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::dispatch(cli).await
}
