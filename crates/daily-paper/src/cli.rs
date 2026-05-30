use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "daily-paper", version, about = "Daily paper recommendation and deep reading pipeline")]
pub struct Cli {
    /// Path to config file
    #[arg(long)]
    pub config: Option<std::path::PathBuf>,

    /// Override state directory
    #[arg(long)]
    pub state_dir: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the full pipeline
    Run(RunArgs),
    /// Show status of recent runs
    Status(StatusArgs),
    /// Serve reports as a web page
    Serve(ServeArgs),
}

#[derive(Parser)]
pub struct RunArgs {
    /// Date to fetch papers for (YYYY-MM-DD)
    #[arg(long)]
    pub date: Option<String>,

    /// Execute pipeline but do not send email
    #[arg(long)]
    pub dry_run: bool,

    /// Force Zotero sync, ignoring cache
    #[arg(long)]
    pub force_zotero_sync: bool,

    /// Force rerank, ignoring cache
    #[arg(long)]
    pub force_rerank: bool,

    /// Force deep read, ignoring cache
    #[arg(long)]
    pub force_read: bool,

    /// Force send, even if report was already sent
    #[arg(long)]
    pub force_send: bool,

    /// Limit number of candidates to process (for testing)
    #[arg(long)]
    pub max_candidates: Option<usize>,
}

#[derive(Parser)]
pub struct StatusArgs {
    /// Show status for a specific run
    #[arg(long)]
    pub run_id: Option<String>,
}

#[derive(Parser, Clone)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(long, default_value = "3000")]
    pub port: u16,
}
