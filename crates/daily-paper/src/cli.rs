use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "daily-paper",
    version,
    about = "Daily paper recommendation and deep reading pipeline"
)]
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

    /// Run only specific stage(s). Can be repeated.
    /// Stages: zotero-sync, source-fetch, deduplicate, embedding, rerank, deep-read, render, send
    #[arg(long = "stage")]
    pub stages: Vec<String>,

    /// When using --stage, load cached data from this run id (defaults to latest run)
    #[arg(long)]
    pub from_run: Option<String>,

    /// Execute pipeline but do not send email (same as default in dev)
    #[arg(long)]
    pub dry_run: bool,

    /// Send the email (dev: off by default; release: on by default)
    #[arg(long)]
    pub send_email: bool,

    /// Skip email even in release mode
    #[arg(long)]
    pub no_email: bool,

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
    /// Override port to listen on (default: from config or 8991)
    #[arg(long)]
    pub port: Option<u16>,
}
