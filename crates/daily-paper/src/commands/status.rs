use std::path::Path;
use crate::cli::StatusArgs;
use tracing::info;

pub async fn execute(state_dir: &Path, args: StatusArgs) -> anyhow::Result<()> {
    info!(state_dir = %state_dir.display(), "checking status");

    if let Some(run_id) = &args.run_id {
        info!(run_id = %run_id, "looking up run");
    } else {
        info!("showing latest run status");
    }

    // TODO: implement status display
    info!("status command not yet implemented");

    Ok(())
}
