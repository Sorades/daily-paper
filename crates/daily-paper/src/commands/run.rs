use std::path::Path;
use anyhow::Context;
use crate::cli::RunArgs;
use daily_paper_core::config::load_config;
use tracing::info;

pub async fn execute(config_path: &Path, state_dir: &Path, args: RunArgs) -> anyhow::Result<()> {
    let (_raw, resolved) = load_config(config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    info!(config = %config_path.display(), "config loaded");
    info!(state_dir = %state_dir.display(), "state directory");
    info!(top_n = resolved.reader.top_n, "pipeline settings");

    if args.dry_run {
        info!("dry-run mode enabled");
    }

    // TODO: implement pipeline stages
    info!("pipeline not yet implemented");

    Ok(())
}
