use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc, Weekday};
use tracing::warn;

use crate::models::common::DateWindow;
use crate::models::run::*;
use crate::state::path::StatePath;
use crate::state::store::FileStateStore;

pub(crate) fn generate_run_id() -> String {
    let now = Local::now();
    let ts = now.format("%Y%m%d-%H%M%S").to_string();
    let suffix: String = (0..6)
        .map(|_| format!("{:x}", fastrand::u8(0..16)))
        .collect();
    format!("{}-{}", ts, suffix)
}

/// arXiv RSS announcement pubDate is midnight US Eastern (UTC-4 during DST).
const ARXIV_ANNOUNCEMENT_UTC_OFFSET_HOURS: i64 = 4;

pub(crate) fn default_arxiv_announcement_date() -> NaiveDate {
    arxiv_announcement_date_for(Utc::now())
}

pub(crate) fn arxiv_announcement_date_for(now: DateTime<Utc>) -> NaiveDate {
    let adjusted_now = now - Duration::hours(ARXIV_ANNOUNCEMENT_UTC_OFFSET_HOURS);
    let today = adjusted_now.date_naive();
    match today.weekday() {
        Weekday::Sun => today - Duration::days(2),
        Weekday::Sat => today - Duration::days(1),
        _ => today,
    }
}

#[allow(dead_code)]
pub(crate) fn compute_date_window(date_arg: Option<&str>) -> anyhow::Result<DateWindow> {
    compute_date_window_with_tz(date_arg, None)
}

pub(crate) fn compute_date_window_with_tz(
    date_arg: Option<&str>,
    timezone_str: Option<&str>,
) -> anyhow::Result<DateWindow> {
    match date_arg {
        Some(d) => {
            let date = NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map_err(|e| anyhow::anyhow!("invalid date format '{}': {}", d, e))?;
            let start = Utc
                .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid start timestamp"))?;
            let end = start + Duration::days(1);
            Ok(DateWindow {
                start,
                end,
                label: d.to_string(),
            })
        }
        None => {
            let offset = crate::commands::serve::scheduler::parse_tz_offset(timezone_str);
            let now_local = Utc::now().with_timezone(&offset);
            let today_local = now_local.date_naive();
            let start = Utc
                .with_ymd_and_hms(today_local.year(), today_local.month(), today_local.day(), 0, 0, 0)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid start timestamp"))?;
            let end = start + Duration::days(1);
            Ok(DateWindow {
                start,
                end,
                label: today_local.to_string(),
            })
        }
    }
}

pub(crate) fn build_cli_overrides(options: &crate::pipeline::PipelineOptions) -> Vec<CliOverride> {
    let mut overrides = Vec::new();
    if options.dry_run {
        overrides.push(CliOverride {
            key: "dry_run".into(),
            value: "true".into(),
        });
    }
    if options.send_email {
        overrides.push(CliOverride {
            key: "send_email".into(),
            value: "true".into(),
        });
    }
    if options.no_email {
        overrides.push(CliOverride {
            key: "no_email".into(),
            value: "true".into(),
        });
    }
    if options.force_zotero_sync {
        overrides.push(CliOverride {
            key: "force_zotero_sync".into(),
            value: "true".into(),
        });
    }
    if options.force_embedding {
        overrides.push(CliOverride {
            key: "force_embedding".into(),
            value: "true".into(),
        });
    }
    if options.force_rerank {
        overrides.push(CliOverride {
            key: "force_rerank".into(),
            value: "true".into(),
        });
    }
    if options.force_read {
        overrides.push(CliOverride {
            key: "force_read".into(),
            value: "true".into(),
        });
    }
    if options.force_send {
        overrides.push(CliOverride {
            key: "force_send".into(),
            value: "true".into(),
        });
    }
    overrides
}

pub(crate) fn error_to_kind(e: &anyhow::Error) -> ErrorKind {
    if let Some(err) = e.downcast_ref::<crate::error::Error>() {
        return err.kind();
    }
    ErrorKind::Storage
}

pub(crate) fn record_stage(manifest: &mut RunManifest, record: StageRecord) {
    if let Some(existing) = manifest.stages.iter_mut().find(|s| s.stage == record.stage) {
        *existing = record;
    } else {
        manifest.stages.push(record);
    }
}

pub(crate) fn mark_stage_blocked(manifest: &mut RunManifest, message: &str) {
    warn!("{}", message);
    add_warning(manifest, "pipeline", message);
}

pub(crate) fn add_warning(manifest: &mut RunManifest, kind: &str, message: &str) {
    manifest.warnings.push(WarningRecord {
        kind: kind.to_string(),
        message: message.to_string(),
        context: serde_json::Value::Null,
    });
}

pub(crate) fn emit_event(
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    manifest: &mut RunManifest,
    store: &FileStateStore,
    manifest_path: &StatePath,
    run_id: &str,
    stage: StageName,
    _is_start: bool,
) {
    if !manifest.stages.iter().any(|s| s.stage == stage) {
        record_stage(
            manifest,
            StageRecord {
                stage: stage.clone(),
                status: StageStatus::Running,
                started_at: Utc::now(),
                finished_at: None,
                cache_hit: false,
                input_hash: None,
                output_ref: None,
                error: None,
            },
        );
        let _ = store.write_json(manifest_path, manifest);
    }

    if let Some(tx) = event_tx {
        let _ = tx.send(PipelineEvent::StageStart {
            run_id: run_id.to_string(),
            stage,
        });
    }
}

pub(crate) fn emit_stage_end<T>(
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    manifest: &RunManifest,
    run_id: &str,
    stage: StageName,
    result: &anyhow::Result<T>,
) {
    if let Some(tx) = event_tx {
        let record = manifest.stages.iter().find(|s| s.stage == stage);
        let (status, cache_hit, duration_ms) = if let Some(r) = record {
            let ms = r
                .finished_at
                .map(|f| (f - r.started_at).num_milliseconds().max(0) as u64)
                .unwrap_or(0);
            (r.status.clone(), r.cache_hit, ms)
        } else {
            let status = if result.is_ok() {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            };
            (status, false, 0u64)
        };
        let _ = tx.send(PipelineEvent::StageEnd {
            run_id: run_id.to_string(),
            stage,
            status,
            cache_hit,
            duration_ms,
        });
    }
}

pub(crate) fn vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) fn bytes_to_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Execution context grouping shared pipeline runtime dependencies.
pub struct ExecutionContext<'a> {
    pub store: &'a FileStateStore,
    pub config: &'a crate::config::ResolvedConfig,
    pub manifest: &'a mut RunManifest,
    pub run_id: &'a str,
    pub date_window: &'a DateWindow,
    pub stage_filter: Option<&'a [StageName]>,
    pub source_manifest: Option<&'a RunManifest>,
    pub event_tx: Option<&'a tokio::sync::broadcast::Sender<PipelineEvent>>,
    pub manifest_path: &'a StatePath,
}

impl<'a> ExecutionContext<'a> {
    pub fn date(&self) -> &str {
        &self.date_window.label
    }

    pub fn cache_date(&self) -> &str {
        self.source_manifest
            .map(|m| m.date_window.label.as_str())
            .unwrap_or_else(|| self.date())
    }

    pub fn should_run(&self, stage: &StageName) -> bool {
        self.stage_filter.map(|f| f.contains(stage)).unwrap_or(true)
    }

    pub fn emit_stage_start(&mut self, stage: StageName) {
        emit_event(
            self.event_tx,
            self.manifest,
            self.store,
            self.manifest_path,
            self.run_id,
            stage,
            true,
        );
    }

    pub fn emit_stage_finish<T>(&mut self, stage: StageName, result: &anyhow::Result<T>) {
        if result.is_err() {
            if let Some(r) = self.manifest.stages.iter_mut().find(|s| s.stage == stage) {
                if r.status == StageStatus::Running {
                    r.status = StageStatus::Failed;
                    r.finished_at = Some(Utc::now());
                }
            }
        }
        emit_stage_end(self.event_tx, self.manifest, self.run_id, stage, result);
    }

    pub fn save_manifest(&self) -> anyhow::Result<()> {
        self.store.write_json(self.manifest_path, self.manifest)?;
        Ok(())
    }
}
