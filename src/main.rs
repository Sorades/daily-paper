use clap::Parser;
use daily_paper::cli::Cli;
use daily_paper::commands;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::dispatch(cli).await
}
