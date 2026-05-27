use std::path::Path;

use anyhow::Context;
use tracing::info;

use crate::cli::StatusArgs;
use daily_paper_core::models::run::{RunManifest, RunStatus, StageName, StageStatus};
use daily_paper_core::state::store::FileStateStore;

pub async fn execute(state_dir: &Path, args: StatusArgs) -> anyhow::Result<()> {
    let store = FileStateStore::new(state_dir.to_path_buf());

    if let Some(run_id) = &args.run_id {
        show_run(&store, run_id)?;
    } else {
        show_latest_run(&store)?;
    }

    Ok(())
}

fn show_latest_run(store: &FileStateStore) -> anyhow::Result<()> {
    let runs_dir = store.root().join("state/runs");
    if !runs_dir.exists() {
        info!("no runs found");
        return Ok(());
    }

    let mut runs: Vec<(String, std::time::SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(&runs_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                let modified = manifest_path
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                let name = entry.file_name().to_string_lossy().to_string();
                runs.push((name, modified));
            }
        }
    }

    if runs.is_empty() {
        info!("no runs found");
        return Ok(());
    }

    runs.sort_by(|a, b| b.1.cmp(&a.1));
    let latest_id = &runs[0].0;
    show_run(store, latest_id)
}

fn show_run(store: &FileStateStore, run_id: &str) -> anyhow::Result<()> {
    let manifest_path = format!("state/runs/{}/manifest.json", run_id);
    let manifest: RunManifest = store
        .read_json(&daily_paper_core::state::path::StatePath::new(
            &manifest_path,
        )?)?
        .context(format!("run '{}' not found", run_id))?;

    println!("Run: {}", manifest.run_id);
    println!(
        "Status: {}",
        match manifest.status {
            RunStatus::Running => "Running",
            RunStatus::Succeeded => "Succeeded",
            RunStatus::Failed => "Failed",
            RunStatus::Blocked => "Blocked",
            RunStatus::Cancelled => "Cancelled",
        }
    );
    println!(
        "Started: {}",
        manifest.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(finished) = manifest.finished_at {
        let duration = finished - manifest.started_at;
        println!(
            "Finished: {} ({}s)",
            finished.format("%Y-%m-%d %H:%M:%S UTC"),
            duration.num_seconds()
        );
    }
    println!("Date Window: {}", manifest.date_window.label);

    if !manifest.stages.is_empty() {
        println!("\nStages:");
        for stage in &manifest.stages {
            let icon = match stage.status {
                StageStatus::Succeeded => {
                    if stage.cache_hit {
                        "cached"
                    } else {
                        "done"
                    }
                }
                StageStatus::Failed => "FAIL",
                StageStatus::Skipped => "skip",
                StageStatus::Running => "...",
                StageStatus::Pending => "---",
            };
            let stage_name = match stage.stage {
                StageName::ZoteroSync => "ZoteroSync   ",
                StageName::SourceFetch => "SourceFetch  ",
                StageName::Deduplicate => "Deduplicate  ",
                StageName::Embedding => "Embedding    ",
                StageName::Rerank => "Rerank       ",
                StageName::PdfFetch => "PdfFetch     ",
                StageName::TextExtract => "TextExtract  ",
                StageName::MetadataFetch => "MetadataFetch",
                StageName::DeepRead => "DeepRead     ",
                StageName::Render => "Render       ",
                StageName::Send => "Send         ",
            };
            print!("  {} {}", stage_name, icon);
            if let Some(ref err) = stage.error {
                print!(" - {}", err.message);
            }
            println!();
        }
    }

    if !manifest.warnings.is_empty() {
        println!("\nWarnings:");
        for w in &manifest.warnings {
            println!("  [{}] {}", w.kind, w.message);
        }
    }

    if let Some(ref err) = manifest.error {
        println!("\nError: [{}] {}", format!("{:?}", err.kind), err.message);
    }

    Ok(())
}
