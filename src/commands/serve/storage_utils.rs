use std::path::Path;
use chrono::NaiveDate;
use tracing::info;

use crate::models::run::{ErrorKind, ErrorRecord, RunManifest, RunStatus};
use crate::state::keys::StorageKey;
use crate::state::store::FileStateStore;

/// Collect all run manifests sorted by started_at descending.
pub(crate) fn collect_runs(store: &FileStateStore) -> Vec<RunManifest> {
    let runs_dir = match StorageKey::RunsDir.to_state_path() {
        Ok(sp) => sp.resolve(store.root()),
        Err(_) => return Vec::new(),
    };
    let mut runs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                let run_id = entry.file_name().to_string_lossy().to_string();
                let key = StorageKey::RunManifest { run_id: &run_id };
                if let Ok(Some(m)) = store.get_json::<RunManifest>(&key) {
                    runs.push(m);
                }
            }
        }
    }
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    runs
}

/// On server startup, mark any runs left in "Running" status from a prior killed/crashed process as "Failed".
/// Also remove any stale lock file left behind.
pub(crate) fn reconcile_orphan_runs(store: &FileStateStore) {
    let runs_dir = match StorageKey::RunsDir.to_state_path() {
        Ok(sp) => sp.resolve(store.root()),
        Err(_) => return,
    };
    if runs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&runs_dir) {
            for entry in entries.flatten() {
                let manifest_path = entry.path().join("manifest.json");
                if manifest_path.exists() {
                    let run_id = entry.file_name().to_string_lossy().to_string();
                    let key = StorageKey::RunManifest { run_id: &run_id };
                    if let Ok(Some(mut m)) = store.get_json::<RunManifest>(&key) {
                        if m.status == RunStatus::Running {
                            info!(run_id = %m.run_id, "marking interrupted run as Failed");
                            m.status = RunStatus::Failed;
                            m.finished_at = Some(chrono::Utc::now());
                            m.error = Some(ErrorRecord {
                                kind: ErrorKind::Storage,
                                message: "process was terminated while running".to_string(),
                                retryable: true,
                                context: serde_json::Value::Null,
                                occurred_at: chrono::Utc::now(),
                            });
                            let _ = store.put_json(&key, &m);
                        }
                    }
                }
            }
        }
    }

    let lock_file = store.root().join("lock");
    if lock_file.exists() {
        let _ = std::fs::remove_file(&lock_file);
    }
}

/// Recursively compute directory size and file count with depth limit and symlink skipping.
pub(crate) fn dir_size_and_count(path: &Path) -> (u64, usize) {
    dir_size_and_count_bounded(path, 0, 10)
}

fn dir_size_and_count_bounded(path: &Path, depth: usize, max_depth: usize) -> (u64, usize) {
    if depth > max_depth {
        return (0, 0);
    }
    let mut size = 0u64;
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            // Check file type to ignore symlinks and avoid loops
            if let Ok(ft) = entry.file_type() {
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    let (sub_size, sub_count) = dir_size_and_count_bounded(&entry.path(), depth + 1, max_depth);
                    size += sub_size;
                    count += sub_count;
                } else if let Ok(meta) = entry.metadata() {
                    size += meta.len();
                    count += 1;
                }
            }
        }
    }
    (size, count)
}

/// Format bytes into human-readable representation.
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Calculate days in month handling leap years.
pub(crate) fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let this_month = NaiveDate::from_ymd_opt(year, month, 1)?;
    Some((next_month - this_month).num_days() as u32)
}
